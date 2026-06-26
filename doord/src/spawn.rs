//! Launching the chosen session as the authenticated user.
//!
//! This is the privilege handoff: the daemon forks, sheds every scrap of its own
//! authority in the child ([`privdrop::drop_to`]), hands that child a clean
//! environment built from scratch ([`privdrop::sanitized_env`]), and `exec`s the
//! session's `Exec=` command. After this the running session has exactly the
//! authenticated user's identity and nothing of root's.
//!
//! Two things are deliberately *not* here yet, because they are the next
//! milestone's job and a different axis: seat/VT ownership and the logind session
//! registration (`XDG_SESSION_*`, the controlling tty, `setsid`). Until that
//! lands the session inherits the daemon's stdio — which is exactly what makes
//! the privilege-drop demonstrable (run `id` as the session and watch it report
//! the user's uid). The launch is split behind the [`SessionLauncher`] trait so
//! the IPC layer's auth-gating can be tested without forking or root, mirroring
//! the [`Authenticator`](crate::pam::Authenticator) seam.

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus};

use crate::privdrop::{self, TargetUser};
use crate::sessions::DiscoveredSession;
use crate::user;

/// Why a session could not be launched. The detail is journaled by the daemon;
/// the greeter only ever sees a generic refusal (it must not learn, say, whether
/// a username exists from a spawn error).
#[derive(Debug)]
pub enum LaunchError {
    /// The authenticated user could not be resolved in the passwd database.
    User(io::Error),
    /// Fork/privilege-drop/exec failed. A drop failure surfaces here too:
    /// [`Command::spawn`] reports a `pre_exec` error, so a session whose
    /// privilege drop did not take is never exec'd.
    Spawn(io::Error),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::User(e) => write!(f, "resolving the user failed: {e}"),
            LaunchError::Spawn(e) => write!(f, "starting the session failed: {e}"),
        }
    }
}

/// A launched session the daemon owns until it exits. Wraps the child process so
/// the IPC layer can wait on (and reap) it after telling the greeter the session
/// started. A childless handle waits trivially — used by the test launcher seam,
/// which records the launch without forking.
pub struct SessionChild(Option<Child>);

impl SessionChild {
    /// A handle with no underlying child; [`wait`](Self::wait) returns `None`.
    #[cfg(test)]
    pub fn detached() -> Self {
        SessionChild(None)
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

/// What turns an authenticated user + chosen session into a running session.
/// Abstracted so the IPC layer's "only after auth, only as the authed user"
/// gating can be exercised in-process without forking or privilege.
pub trait SessionLauncher {
    /// Resolve the authenticated `username`, drop to it, and exec `session`'s
    /// command. Returns once the child has successfully `exec`'d (a privilege
    /// drop or exec failure is an `Err`, never a half-started session).
    fn launch(&self, session: &DiscoveredSession, username: &str)
        -> Result<SessionChild, LaunchError>;
}

/// The production launcher: a real fork + privilege drop + exec.
pub struct ProcessLauncher;

impl SessionLauncher for ProcessLauncher {
    fn launch(
        &self,
        session: &DiscoveredSession,
        username: &str,
    ) -> Result<SessionChild, LaunchError> {
        let target = user::resolve(username).map_err(LaunchError::User)?;

        let mut cmd = build_command(session, &target);

        // The privilege drop runs in the child, after fork, before exec. The
        // process is single-threaded at fork (the daemon serves one greeter
        // sequentially with no worker threads), so the child is the only thread
        // and this non-async-signal-safe work (initgroups reads /etc/group) is
        // safe here — the same shape login(1) and sshd use. If the drop fails,
        // returning Err aborts the exec, so a session is never run with residual
        // privilege.
        //
        // SAFETY: the closure only calls drop_to against an owned TargetUser it
        // captured; it touches no shared state of the parent and returns its
        // result so a failed drop fails the spawn.
        unsafe {
            cmd.pre_exec(move || privdrop::drop_to(&target));
        }

        let child = cmd.spawn().map_err(LaunchError::Spawn)?;
        Ok(SessionChild(Some(child)))
    }
}

/// Build the child command's *configuration* — program, arguments, environment,
/// and working directory — but not the privilege drop (which is wired in by the
/// launcher as a `pre_exec` step). Pure and side-effect-free so it can be
/// asserted on in tests without forking.
///
/// The program is the session's tokenized `Exec=`: a bare name like `Hyprland`
/// or a wrapper launcher like `start-hyprland` is resolved against the sanitized
/// `PATH` we set below (where the session binaries live), and an absolute path is
/// used as-is. The environment is cleared and rebuilt from the allowlist so none
/// of the daemon's own environment (its `PATH`, any inherited `LD_*`) can leak
/// into the user session. The working directory is the user's home.
fn build_command(session: &DiscoveredSession, target: &TargetUser) -> Command {
    let mut cmd = Command::new(&session.exec[0]);
    cmd.args(&session.exec[1..]);
    cmd.env_clear();
    cmd.envs(privdrop::sanitized_env(target));
    cmd.current_dir(&target.home);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionKind;
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

    fn session(exec: &[&str]) -> DiscoveredSession {
        DiscoveredSession {
            id: "test".to_string(),
            name: "Test".to_string(),
            comment: None,
            exec: exec.iter().map(|s| s.to_string()).collect(),
            kind: SessionKind::Wayland,
        }
    }

    #[test]
    fn command_honors_exec_program_and_args() {
        // A wrapper launcher with arguments is passed through verbatim; the bare
        // program name is left for PATH resolution against the sanitized PATH.
        let cmd = build_command(&session(&["start-hyprland", "--foo", "bar"]), &target());
        assert_eq!(cmd.get_program(), OsStr::new("start-hyprland"));
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, vec![OsStr::new("--foo"), OsStr::new("bar")]);
    }

    #[test]
    fn command_runs_in_the_users_home() {
        let cmd = build_command(&session(&["Hyprland"]), &target());
        assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/home/stephen")));
    }

    #[test]
    fn command_environment_is_the_sanitized_allowlist_only() {
        let cmd = build_command(&session(&["Hyprland"]), &target());
        // env_clear means get_envs reports the full child environment: every
        // entry has a value (none are inherited/removed), and the set is exactly
        // the allowlist bound to the target.
        let env: HashMap<String, String> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.expect("every entry is explicitly set").to_string_lossy().into_owned(),
                )
            })
            .collect();

        let mut keys: Vec<&String> = env.keys().collect();
        keys.sort();
        assert_eq!(keys, ["HOME", "LOGNAME", "PATH", "SHELL", "USER"]);
        assert_eq!(env["HOME"], "/home/stephen");
        assert_eq!(env["USER"], "stephen");
        assert_eq!(env["LOGNAME"], "stephen");
        assert_eq!(env["SHELL"], "/bin/zsh");
        assert!(!env["PATH"].is_empty());
        // The daemon's own PATH/LD_* never leak in.
        assert!(!env.contains_key("LD_PRELOAD"));
    }
}
