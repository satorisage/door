//! Launching the chosen session as the authenticated user.
//!
//! This is the privilege handoff: the daemon forks, the child takes the seat's
//! VT as its controlling terminal, sheds every scrap of the daemon's authority
//! ([`privdrop::drop_to`]), runs in an environment built from the sanitized
//! allowlist merged with the PAM session environment ([`build_command`]), and
//! `exec`s the session's `Exec=` command. After this the running session has
//! exactly the authenticated user's identity and nothing of the daemon's.
//!
//! The PAM session itself (the logind registration via `pam_systemd`, which
//! produces the `XDG_SESSION_*` environment merged in here) is opened by the
//! caller in the daemon and held open across this spawn; this module is only the
//! fork → VT handoff → privilege drop → exec mechanism. It is a free function
//! rather than a trait: the per-login PAM transaction in [`crate::pam`] owns the
//! decision of *when* to launch (only after auth, only as the authed user) and
//! the substitution seam for tests, because that decision needs the live PAM
//! context this layer does not hold.

use std::ffi::{CString, OsString};
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus};

use crate::config::SeatTarget;
use crate::privdrop::{self, TargetUser};

/// Why a session could not be launched. The detail is journaled by the daemon;
/// the greeter only ever sees a generic refusal (it must not learn, say, whether
/// a username exists from a spawn error).
#[derive(Debug)]
pub enum LaunchError {
    /// Fork / VT-handoff / privilege-drop / exec failed. A drop or VT failure
    /// surfaces here too: [`Command::spawn`] reports a `pre_exec` error, so a
    /// session whose privilege drop or seat handoff did not take is never exec'd.
    Spawn(io::Error),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::Spawn(e) => write!(f, "starting the session failed: {e}"),
        }
    }
}

/// A launched session the daemon owns until it exits. Wraps the child process so
/// the caller can wait on (and reap) it after telling the greeter the session
/// started. A childless handle waits trivially — used by the test login seam,
/// which records the launch without forking.
pub struct SessionChild(Option<Child>);

impl SessionChild {
    /// Wrap a child process the caller already spawned (the session worker) as a
    /// waitable session handle.
    pub fn new(child: Child) -> Self {
        SessionChild(Some(child))
    }

    /// A handle with no underlying child; [`wait`](Self::wait) returns `None`.
    #[cfg(test)]
    pub fn detached() -> Self {
        SessionChild(None)
    }

    /// The child's pid, if there is one. The greeter path needs it to forward a
    /// terminate to the compositor at handoff (a childless test handle has none).
    pub fn id(&self) -> Option<u32> {
        self.0.as_ref().map(Child::id)
    }

    /// Block until the session exits, reaping it. Returns its exit status, or
    /// `None` for a childless handle.
    pub fn wait(mut self) -> io::Result<Option<ExitStatus>> {
        match self.0.take() {
            Some(mut child) => child.wait().map(Some),
            None => Ok(None),
        }
    }
}

/// Fork the chosen session for `target`, hand it the seat's VT, drop privilege,
/// and exec. `pam_env` is the PAM session environment (the `XDG_SESSION_*` /
/// `XDG_RUNTIME_DIR` logind set when the session was opened), merged over the
/// sanitized allowlist. Returns once the child has successfully `exec`'d; a
/// VT-handoff, privilege-drop, or exec failure is an `Err`, never a half-started
/// session.
///
/// `parent_death_signal`, when set, arms `PR_SET_PDEATHSIG` on the child so it is
/// signalled if its parent (the spawning worker) dies — used by the greeter so a
/// compositor holding seat0 can never be orphaned onto the VT. The session path
/// passes `None`.
pub fn launch(
    exec: &[String],
    target: &TargetUser,
    pam_env: &[(OsString, OsString)],
    seat: &SeatTarget,
    parent_death_signal: Option<libc::c_int>,
) -> Result<SessionChild, LaunchError> {
    let mut cmd = build_command(exec, target, pam_env);

    // Precompute the VT device path as a C string *before* fork, so the
    // post-fork `pre_exec` does no allocation it doesn't have to. `None` (a dev
    // run with no VT) means the child keeps the inherited stdio.
    let tty_cpath = match seat.vtnr {
        Some(vtnr) => Some(
            CString::new(format!("/dev/tty{vtnr}"))
                .map_err(|e| LaunchError::Spawn(io::Error::new(io::ErrorKind::InvalidInput, e)))?,
        ),
        None => None,
    };

    let target = target.clone();

    // The session setup runs in the child, after fork, before exec. The process
    // is single-threaded at fork (the daemon serves one greeter sequentially with
    // no worker threads), so the child is the only thread and this
    // non-async-signal-safe work (initgroups reads /etc/group; the VT ioctls) is
    // safe here — the same shape login(1) and the display managers use. Any
    // failure returns Err, which aborts the exec, so a session is never run with
    // residual privilege or off its seat.
    //
    // SAFETY: the closure touches only owned values it captured (the target and
    // the precomputed tty path) and process-global state of the fresh child; it
    // shares nothing with the parent and returns its result so a failed setup
    // fails the spawn.
    unsafe {
        cmd.pre_exec(move || session_setup(&target, tty_cpath.as_deref(), parent_death_signal));
    }

    let child = cmd.spawn().map_err(LaunchError::Spawn)?;
    Ok(SessionChild(Some(child)))
}

/// Post-fork, pre-exec child setup: become a session leader, take the VT as the
/// controlling terminal and route stdio to it (while still privileged enough to
/// open the VT device), then drop to the target user. Ordering is load-bearing:
/// the VT is opened *before* the uid drop because the device is root-owned until
/// logind reassigns it, and the privilege drop is *last* because after it we can
/// no longer change groups or gid.
fn session_setup(
    target: &TargetUser,
    tty_cpath: Option<&std::ffi::CStr>,
    parent_death_signal: Option<libc::c_int>,
) -> io::Result<()> {
    // New session: detach from the daemon's session and controlling tty so the
    // user's session leads its own. For a forked child this cannot fail.
    // SAFETY: setsid takes no arguments; the child is not already a group leader.
    if unsafe { libc::setsid() } == -1 {
        return Err(io::Error::last_os_error());
    }

    if let Some(tty_cpath) = tty_cpath {
        take_controlling_tty(tty_cpath)?;
    }

    // Privilege drop before the parent-death signal (see fn doc): groups → gid →
    // uid, verified, refuses uid 0. After this the child is fully the target user.
    privdrop::drop_to(target)?;

    // Parent-death signal LAST of all. It must be armed *after* the uid/gid drop:
    // the kernel clears any pending PR_SET_PDEATHSIG whenever the effective user or
    // group id changes, so arming it before privdrop would silently wipe it. With
    // it set, the child is signalled the instant its parent (the spawning worker)
    // dies — so a compositor that holds seat0 can never be orphaned onto the VT if
    // that worker is killed before it can tear the compositor down cleanly.
    if let Some(sig) = parent_death_signal {
        // SAFETY: sets this process's own parent-death signal; scalar args, no
        // shared state.
        if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, sig as libc::c_ulong, 0, 0, 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // Close the fork→arm race: if the parent already exited, the kernel will
        // never deliver the death signal, so fail the spawn now rather than launch
        // a compositor that would outlive the worker meant to own it.
        // SAFETY: getppid takes no arguments and cannot fail.
        if unsafe { libc::getppid() } == 1 {
            return Err(io::Error::other(
                "parent exited before the parent-death signal was armed",
            ));
        }
    }

    Ok(())
}

/// Open the seat's VT, make it this session's controlling terminal, and route
/// stdin/stdout/stderr to it — replacing the daemon's inherited stdio so the
/// session owns the seat rather than the daemon's pipes.
fn take_controlling_tty(tty_cpath: &std::ffi::CStr) -> io::Result<()> {
    // O_NOCTTY: opening does not implicitly grab the tty; we claim it explicitly
    // below so the semantics are the same whether or not the kernel would have.
    // SAFETY: tty_cpath is a valid NUL-terminated path; open returns an owned fd
    // or -1.
    let fd = unsafe { libc::open(tty_cpath.as_ptr(), libc::O_RDWR | libc::O_NOCTTY) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    // Claim the VT as this (newly-led) session's controlling terminal.
    // SAFETY: fd is a freshly opened tty; TIOCSCTTY with arg 0 sets it controlling.
    if unsafe { libc::ioctl(fd, libc::TIOCSCTTY, 0) } != 0 {
        let e = io::Error::last_os_error();
        // SAFETY: fd is open and owned here.
        unsafe { libc::close(fd) };
        return Err(e);
    }

    // Point std{in,out,err} at the VT.
    for std_fd in 0..3 {
        // SAFETY: dup2 onto the three standard fds from a valid open fd.
        if unsafe { libc::dup2(fd, std_fd) } < 0 {
            let e = io::Error::last_os_error();
            // SAFETY: fd is open and owned here.
            unsafe { libc::close(fd) };
            return Err(e);
        }
    }
    if fd > 2 {
        // SAFETY: the original fd is now duplicated onto 0..3 and no longer needed.
        unsafe { libc::close(fd) };
    }
    Ok(())
}

/// Build the child command's *configuration* — program, arguments, environment,
/// and working directory — but not the VT handoff or privilege drop (those are
/// wired in by [`launch`] as `pre_exec` steps). Pure and side-effect-free so it
/// can be asserted on in tests without forking.
///
/// The program is the session's tokenized `Exec=`: a bare name like `Hyprland`
/// or a wrapper launcher like `start-hyprland` is resolved against the sanitized
/// `PATH` we set below, and an absolute path is used as-is. The environment is
/// cleared and rebuilt from the allowlist, then the PAM session environment is
/// merged *over* it: `pam_env` (the `XDG_SESSION_*` / `XDG_RUNTIME_DIR` logind
/// produced) wins on overlapping keys, but none of the daemon's own environment
/// (its `PATH`, any inherited `LD_*`) can leak in, because we never copy it. The
/// working directory is the user's home.
fn build_command(
    exec: &[String],
    target: &TargetUser,
    pam_env: &[(OsString, OsString)],
) -> Command {
    let mut cmd = Command::new(&exec[0]);
    cmd.args(&exec[1..]);
    cmd.env_clear();
    cmd.envs(privdrop::sanitized_env(target));
    // Merged last so logind's session vars take precedence on any shared key.
    cmd.envs(pam_env.iter().map(|(k, v)| (k.clone(), v.clone())));
    cmd.current_dir(&target.home);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsStr;

    fn target() -> TargetUser {
        TargetUser {
            name: "stephen".to_string(),
            uid: 1000,
            gid: 1000,
            home: "/home/stephen".to_string(),
            shell: "/bin/zsh".to_string(),
        }
    }

    fn exec(argv: &[&str]) -> Vec<String> {
        argv.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn command_honors_exec_program_and_args() {
        // A wrapper launcher with arguments is passed through verbatim; the bare
        // program name is left for PATH resolution against the sanitized PATH.
        let cmd = build_command(&exec(&["start-hyprland", "--foo", "bar"]), &target(), &[]);
        assert_eq!(cmd.get_program(), OsStr::new("start-hyprland"));
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, vec![OsStr::new("--foo"), OsStr::new("bar")]);
    }

    #[test]
    fn command_runs_in_the_users_home() {
        let cmd = build_command(&exec(&["Hyprland"]), &target(), &[]);
        assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/home/stephen")));
    }

    fn env_of(cmd: &Command) -> HashMap<String, String> {
        cmd.get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.expect("every entry is explicitly set").to_string_lossy().into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn command_environment_is_the_sanitized_allowlist_only_without_pam_env() {
        let cmd = build_command(&exec(&["Hyprland"]), &target(), &[]);
        // env_clear means get_envs reports the full child environment: every
        // entry has a value (none inherited/removed). The set is the identity/PATH
        // allowlist bound to the target, plus locale (system config, passed
        // through deliberately) — and nothing else inherited.
        let env = env_of(&cmd);
        for required in ["HOME", "USER", "LOGNAME", "SHELL", "PATH"] {
            assert!(env.contains_key(required), "missing allowlist key: {required}");
        }
        let locale = [
            "LANG", "LANGUAGE", "LC_ALL", "LC_CTYPE", "LC_NUMERIC", "LC_TIME", "LC_COLLATE",
            "LC_MONETARY", "LC_MESSAGES", "LC_PAPER", "LC_NAME", "LC_ADDRESS", "LC_TELEPHONE",
            "LC_MEASUREMENT", "LC_IDENTIFICATION",
        ];
        let allowed = ["HOME", "USER", "LOGNAME", "SHELL", "PATH"];
        for key in env.keys() {
            assert!(
                allowed.contains(&key.as_str()) || locale.contains(&key.as_str()),
                "unexpected inherited key leaked into the session: {key}"
            );
        }
        assert_eq!(env["HOME"], "/home/stephen");
        assert_eq!(env["USER"], "stephen");
        assert_eq!(env["LOGNAME"], "stephen");
        assert_eq!(env["SHELL"], "/bin/zsh");
        assert!(!env["PATH"].is_empty());
        // The daemon's own PATH/LD_* never leak in.
        assert!(!env.contains_key("LD_PRELOAD"));
    }

    #[test]
    fn pam_session_env_is_merged_and_wins_on_overlap() {
        // logind's session vars are admitted; on a shared key (PATH) the PAM
        // value takes precedence, but LD_* still cannot appear because nothing is
        // inherited — only what PAM/the allowlist explicitly provide.
        let pam_env = vec![
            (OsString::from("XDG_SESSION_ID"), OsString::from("7")),
            (OsString::from("XDG_RUNTIME_DIR"), OsString::from("/run/user/1000")),
            (OsString::from("PATH"), OsString::from("/pam/bin")),
        ];
        let cmd = build_command(&exec(&["Hyprland"]), &target(), &pam_env);
        let env = env_of(&cmd);
        assert_eq!(env["XDG_SESSION_ID"], "7");
        assert_eq!(env["XDG_RUNTIME_DIR"], "/run/user/1000");
        // PAM's PATH overrode the allowlist's.
        assert_eq!(env["PATH"], "/pam/bin");
        // Identity from the allowlist is still present.
        assert_eq!(env["USER"], "stephen");
        assert!(!env.contains_key("LD_PRELOAD"));
    }
}
