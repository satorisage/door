//! The local IPC server: the daemon's side of the trust boundary.
//!
//! This is where untrusted bytes first reach the privileged core, so the
//! authorization checks live here, before any request is interpreted:
//!
//! - a **pathname** Unix socket (never the abstract namespace, which has no
//!   filesystem permissions) under a `root`-owned `0700` directory, with the
//!   socket itself `0660` so only `root` and the greeter group can reach it;
//! - an `SO_PEERCRED` check on every connection — the kernel-attested uid of the
//!   peer must match the configured greeter uid, regardless of socket
//!   permissions (defense in depth);
//! - **one** greeter at a time: connections are served sequentially, so there is
//!   never a race between two greeters over one seat;
//! - a **read timeout** so a peer that opens a connection and goes silent cannot
//!   pin the daemon;
//! - a mandatory **version handshake** ([`PROTOCOL_VERSION`]) before any other
//!   request is honored.
//!
//! Above the per-connection handling sits the **login loop** ([`serve`], D-0008):
//! when a greeter user is configured, doord owns the greeter lifecycle — it
//! launches the greeter, serves the login, performs the greeter→session VT handoff
//! (tears the greeter down before the session takes the seat), waits for the
//! session to end, and re-greets. Without a greeter user (dev), it just serves
//! whatever greeter connects.

use std::cell::RefCell;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use protocol::{read_frame, write_frame, FrameError, Request, Response, PROTOCOL_VERSION};

use crate::config::Config;
use crate::pam::{AuthOutcome, Login, LoginFactory};
use crate::worker;

/// How long to wait for the greeter to exit after `SIGTERM` before `SIGKILL`.
/// The greeter-worker does real teardown in this window — it forwards the
/// terminate to its compositor (which releases seat0's DRM/VT) and then closes the
/// greeter logind session — so the grace must cover a DRM teardown plus a session
/// close, not just a process exit. If it overruns, the `SIGKILL` lands and the
/// compositor's `PR_SET_PDEATHSIG` still guarantees it dies and frees the VT.
const GREETER_TERM_GRACE: Duration = Duration::from_secs(2);

/// A managed greeter that lives less than this clearly failed (it never carried a
/// real login, which lasts a session); re-greeting it immediately would spin. Back
/// off [`GREETER_RESPAWN_BACKOFF`] before trying again.
const GREETER_MIN_UPTIME: Duration = Duration::from_secs(3);
const GREETER_RESPAWN_BACKOFF: Duration = Duration::from_secs(2);

/// After this many *consecutive* quick greeter failures the daemon stops trying
/// and exits cleanly ([`give_up`]). Relaunching a compositor (cage) that grabs
/// DRM/KMS on a tight loop is what wedges the GPU and blacks out every VT, so a
/// sustained failure must fail *safe* — leave a usable text console — not spin.
const GREETER_MAX_RAPID_FAILURES: u32 = 3;

/// How long the daemon waits on a stalled read before dropping the peer.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Floor on how long a *failed* authentication takes to report. A wrong username
/// must not fail visibly faster than a wrong password, or the timing itself
/// leaks which accounts exist. Only the failure path is padded — success is not
/// slowed. Lockout/backoff proper is left to the PAM stack (`pam_faillock`).
const MIN_AUTH_FAILURE: Duration = Duration::from_secs(1);

/// The daemon's main login loop (D-0008). When a greeter user is configured,
/// doord owns the greeter lifecycle: **greet** (launch the greeter), **serve**
/// the login, hand off (tear the greeter down before the session takes the VT),
/// wait for the session to end, and **re-greet**. Without a greeter user (dev),
/// it simply serves whatever greeter connects, forever.
///
/// Returns only on a fatal error setting up the listener; per-connection and
/// per-greeter errors are logged and the loop continues.
pub fn serve(config: &Config, logins: &dyn LoginFactory) -> io::Result<()> {
    let listener = bind(config)?;
    let manage_greeter = config.greeter_user.is_some();
    eprintln!(
        "doord: listening on {} (greeter uid {}, pam '{}', seat '{}' vt {:?}, managed greeter: {})",
        config.socket_path.display(),
        config.greeter_uid,
        config.pam_service,
        config.seat.seat,
        config.seat.vtnr,
        manage_greeter,
    );

    // Consecutive quick-failure streak; reset by any greeter that lasts a real
    // login. A sustained streak trips the give-up path rather than thrashing.
    let mut rapid_failures: u32 = 0;
    loop {
        // Greet: launch the greeter doord manages (re-greet on each loop). If the
        // launch fails, back off and retry rather than block on accept() with no
        // greeter to connect.
        let greeter = if manage_greeter {
            match launch_greeter(config) {
                Ok(child) => {
                    eprintln!("doord: launched greeter (pid {})", child.id());
                    Some(RefCell::new(GreeterHandle::new(child)))
                }
                Err(e) => {
                    eprintln!("doord: could not launch greeter: {e}; retrying shortly");
                    rapid_failures += 1;
                    if rapid_failures >= GREETER_MAX_RAPID_FAILURES {
                        return give_up(config, rapid_failures);
                    }
                    thread::sleep(GREETER_RESPAWN_BACKOFF);
                    continue;
                }
            }
        } else {
            None
        };

        // Serve one greeter connection. Sequential by construction — one seat, one
        // greeter, no concurrency.
        let greeted_at = Instant::now();
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(e) = handle_connection(stream, config, logins, greeter.as_ref()) {
                    eprintln!("doord: connection ended: {e}");
                }
            }
            Err(e) => eprintln!("doord: accept failed: {e}"),
        }

        // Ensure the greeter is gone before re-greeting (idempotent: after a
        // login handoff it is already terminated; after a non-login disconnect or
        // a crash it is reaped here).
        if let Some(greeter) = greeter {
            greeter.borrow_mut().terminate();
            // Crash-loop guard: a managed greeter that barely lived (it never
            // carried a real login) must not be re-greeted in a tight spin. Count
            // the streak; once it crosses the threshold, give up (fail safe) so a
            // broken greeter can't wedge the GPU/VT by being relaunched forever.
            if greeted_at.elapsed() < GREETER_MIN_UPTIME {
                rapid_failures += 1;
                if rapid_failures >= GREETER_MAX_RAPID_FAILURES {
                    return give_up(config, rapid_failures);
                }
                eprintln!(
                    "doord: greeter exited quickly ({rapid_failures}/{GREETER_MAX_RAPID_FAILURES}); backing off before re-greet"
                );
                thread::sleep(GREETER_RESPAWN_BACKOFF);
            } else {
                // A greeter that carried a real login clears the streak.
                rapid_failures = 0;
            }
        }
    }
}

/// Abandon the login loop after the greeter has failed too many times in a row.
/// Restores the seat VT to a usable text console — a crashed or SIGKILLed
/// compositor leaves it in graphics mode with switching wedged — then returns so
/// the daemon exits *cleanly*. With `Type=simple` + `Restart=on-failure`, a clean
/// exit does **not** respawn: doord stops thrashing and the machine stays
/// recoverable from a text VT or SSH instead of locked out. The journal says why.
fn give_up(config: &Config, failures: u32) -> io::Result<()> {
    eprintln!(
        "doord: greeter failed {failures} times in quick succession; giving up to keep the \
         console usable. Check the greeter binary, the socket permissions, and the compositor, \
         then re-enable doord. Restoring the VT and exiting."
    );
    if let Some(vtnr) = config.seat.vtnr {
        restore_text_vt(vtnr);
    }
    Ok(())
}

/// Linux console/VT ioctls the `libc` crate does not expose for Linux. These are
/// stable kernel ABI (`<linux/kd.h>`, `<linux/vt.h>`).
mod vt_ioctl {
    pub const KDSETMODE: libc::Ioctl = 0x4B3A;
    pub const KD_TEXT: libc::c_int = 0x00;
    pub const VT_SETMODE: libc::Ioctl = 0x5602;
    pub const VT_ACTIVATE: libc::Ioctl = 0x5606;
    pub const VT_AUTO: libc::c_char = 0x00;

    /// Mirrors `struct vt_mode` from `<linux/vt.h>`.
    #[repr(C)]
    pub struct VtMode {
        pub mode: libc::c_char,
        pub waitv: libc::c_char,
        pub relsig: libc::c_short,
        pub acqsig: libc::c_short,
        pub frsig: libc::c_short,
    }
}

/// Restore VT `vtnr` to a switchable text console. A compositor (cage) that was
/// SIGKILLed never undoes its own VT setup: it leaves the VT in graphics mode
/// and — the part that actually freezes `Ctrl+Alt+Fn` — in process-controlled
/// switch mode (`VT_PROCESS`), where the kernel waits forever for a switch ack
/// from the dead process. We reset switching to kernel-driven (`VT_AUTO`) and the
/// console to text (`KD_TEXT`), then make the VT current so the user sees it.
/// doord runs as root, so it can open the VT; this is best-effort cleanup on the
/// give-up path, so a failure is logged, not fatal.
fn restore_text_vt(vtnr: u32) {
    let path = format!("/dev/tty{vtnr}");
    let c_path = match std::ffi::CString::new(path.clone()) {
        Ok(c) => c,
        Err(_) => return,
    };
    // SAFETY: c_path is a valid NUL-terminated path; open returns an owned fd or -1.
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_NOCTTY) };
    if fd < 0 {
        eprintln!(
            "doord: could not open {path} to restore the console: {}",
            io::Error::last_os_error()
        );
        return;
    }
    let auto = vt_ioctl::VtMode {
        mode: vt_ioctl::VT_AUTO,
        waitv: 0,
        relsig: 0,
        acqsig: 0,
        frsig: 0,
    };
    // SAFETY: fd is a freshly opened VT we own; each request matches its argument
    // type per the kernel ABI (VT_SETMODE takes a *const vt_mode; KDSETMODE and
    // VT_ACTIVATE take an int). fd is closed exactly once below.
    unsafe {
        // Kernel-driven switching again (undo a dead compositor's VT_PROCESS).
        libc::ioctl(fd, vt_ioctl::VT_SETMODE, &auto as *const vt_ioctl::VtMode);
        // Leave graphics mode so the text console renders.
        libc::ioctl(fd, vt_ioctl::KDSETMODE, vt_ioctl::KD_TEXT);
        // Bring this VT to the foreground so the restored console is visible.
        libc::ioctl(fd, vt_ioctl::VT_ACTIVATE, vtnr as libc::c_int);
        libc::close(fd);
    }
}

/// A managed greeter process (the re-exec'd greeter worker, which runs
/// `cage -- door-greeter`). Owns the child so it can be torn down at the
/// greeter→session handoff and reaped before a re-greet.
struct GreeterHandle {
    child: Option<Child>,
}

impl GreeterHandle {
    fn new(child: Child) -> Self {
        GreeterHandle { child: Some(child) }
    }

    /// Terminate the greeter and **wait for it to exit**, so the seat's VT/DRM is
    /// released before the session takes it. `SIGTERM` first (cage releases the
    /// seat and exits cleanly), escalating to `SIGKILL` if it lingers. Idempotent:
    /// a second call (or one after the greeter already exited) just reaps.
    fn terminate(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let pid = child.id() as libc::pid_t;
        // SAFETY: pid is this child's; SIGTERM is a request to exit.
        unsafe { libc::kill(pid, libc::SIGTERM) };

        let deadline = Instant::now() + GREETER_TERM_GRACE;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Ok(None) => break,
                Err(e) => {
                    eprintln!("doord: waiting on the greeter failed: {e}");
                    return;
                }
            }
        }
        // Still alive after the grace period — force it.
        // SAFETY: pid is this child's; it has not been reaped (try_wait returned None).
        unsafe { libc::kill(pid, libc::SIGKILL) };
        let _ = child.wait();
    }
}

/// Re-exec the daemon as the greeter worker (D-0008). The child inherits the
/// daemon's environment (the `DOORD_*` config), which is how the greeter worker
/// learns the greeter user, seat/VT, PAM service, and command. No socket is
/// passed — the daemon manages this process by pid.
fn launch_greeter(config: &Config) -> io::Result<Child> {
    let _ = config; // configuration travels via the inherited environment
    Command::new("/proc/self/exe")
        .arg(worker::GREETER_WORKER_ARG)
        .spawn()
}

/// Create the socket directory, remove any stale socket, bind, and lock down
/// permissions and ownership.
fn bind(config: &Config) -> io::Result<UnixListener> {
    let path = &config.socket_path;

    if let Some(dir) = path.parent() {
        let created = !dir.exists();
        if created {
            fs::create_dir_all(dir)?;
        }
        // The greeter runs as a *different*, unprivileged user, so it must be able
        // to traverse this directory to reach the socket inside it. The parent
        // dir's mode gates the path *before* the socket's own 0660 is consulted:
        // a 0700 root:root dir — which the unit's `RuntimeDirectory` pre-creates —
        // makes the socket unreachable no matter how the socket itself is chmod'd.
        // So when the greeter group is known (production) we group-own the dir to
        // it with group-search (0750), applied whether or not we created it
        // (RuntimeDirectory runs before us). In a dev run with no greeter group we
        // keep a dir we created private and leave a pre-existing shared dir (e.g.
        // /tmp) exactly as its owner set it.
        match config.greeter_gid {
            Some(gid) => {
                chown_group(dir, gid).unwrap_or_else(|e| {
                    eprintln!(
                        "doord: warning: could not chgrp {} to gid {gid}: {e}",
                        dir.display()
                    );
                });
                fs::set_permissions(dir, fs::Permissions::from_mode(0o750))?;
            }
            None if created => {
                fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
            }
            None => {}
        }
    }

    // A leftover socket from a previous run would make bind() fail with EADDRINUSE.
    if path.exists() {
        fs::remove_file(path)?;
    }

    let listener = UnixListener::bind(path)?;

    // 0660: owner (root) and the greeter group, nobody else.
    fs::set_permissions(path, fs::Permissions::from_mode(0o660))?;

    // Hand group ownership to the greeter group when we know it and can do it
    // (root). Best-effort: in a non-root dev run this is skipped, and the
    // peercred check below is what actually enforces who may connect.
    if let Some(gid) = config.greeter_gid {
        chown_group(path, gid).unwrap_or_else(|e| {
            eprintln!("doord: warning: could not chgrp the socket to gid {gid}: {e}");
        });
    }

    Ok(listener)
}

/// Serve one greeter: authorize it, run the handshake, then answer requests
/// until it disconnects.
fn handle_connection(
    stream: UnixStream,
    config: &Config,
    logins: &dyn LoginFactory,
    managed_greeter: Option<&RefCell<GreeterHandle>>,
) -> Result<(), FrameError> {
    let cred = peer_cred(&stream)?;
    if cred.uid != config.greeter_uid {
        // Wrong peer: refuse before reading a single request byte.
        eprintln!(
            "doord: rejecting connection from uid {} (pid {}); only greeter uid {} is allowed",
            cred.uid, cred.pid, config.greeter_uid
        );
        return Ok(());
    }
    eprintln!("doord: greeter connected (pid {}, uid {})", cred.pid, cred.uid);

    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut conn = stream;

    if !handshake(&mut conn)? {
        return Ok(());
    }

    // The per-connection login owns the PAM transaction (auth → session). It is
    // handed its own clone of the connection so its PAM conversation can prompt
    // the greeter without contending with this loop's request reads (the two
    // never read concurrently: the loop is blocked inside `authenticate` while
    // the conversation runs). `Start` is honored only once the login reports an
    // authenticated user — bound by a completed PAM success, never by anything
    // the greeter names — and the login dies with the connection that earned it.
    let greeter = match conn.try_clone() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("doord: could not clone greeter connection for PAM: {e}");
            return Ok(());
        }
    };
    let mut login = logins.begin(greeter);

    loop {
        let request: Request = match read_frame(&mut conn) {
            Ok(req) => req,
            // A clean disconnect surfaces as EOF on read_exact.
            Err(FrameError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                eprintln!("doord: greeter disconnected");
                return Ok(());
            }
            Err(e) => {
                // Never echo the offending bytes (they may be a mistyped secret).
                eprintln!("doord: dropping greeter after frame error: {e}");
                return Ok(());
            }
        };

        // BeginAuth and Start each do more than a single request → response:
        // BeginAuth runs a multi-step PAM conversation over the connection, and
        // Start launches and then owns the session. The rest are stateless.
        match request {
            Request::BeginAuth { username } => {
                match run_auth(&mut conn, login.as_mut(), &username)? {
                    AuthFlow::Continue => {}
                    AuthFlow::Break => return Ok(()),
                }
            }
            Request::Start { session_id } => {
                // The session-start gate: no auth, no spawn. A compromised greeter
                // cannot reach the privilege handoff without first driving a real
                // PAM success, and then only for the user that success bound.
                if login.user().is_none() {
                    eprintln!("doord: refusing Start: no authenticated user on this connection");
                    write_frame(
                        &mut conn,
                        &Response::Error {
                            message: "authenticate before starting a session".to_string(),
                        },
                    )?;
                } else if run_start(
                    &mut conn,
                    config,
                    login.as_mut(),
                    managed_greeter,
                    &session_id,
                )?
                .is_started()
                {
                    // The session is running and the greeter has stepped aside;
                    // this connection's work is done.
                    return Ok(());
                }
            }
            other => {
                let response = dispatch(&other, config);
                write_frame(&mut conn, &response)?;
            }
        }
    }
}

/// Outcome of an authentication conversation, as it affects the connection. The
/// authenticated identity itself lives in the login, not here — the loop only
/// needs to know whether to keep serving.
enum AuthFlow {
    /// Authenticated, or a retryable failure/cancel; keep serving. On success the
    /// login now reports a user, which the `Start` gate reads.
    Continue,
    /// The greeter vanished mid-conversation; tear the connection down.
    Break,
}

/// Outcome of a `Start` request.
enum StartFlow {
    /// The session launched; the connection is finished.
    Started,
    /// The launch was refused (unknown session, spawn error); keep serving.
    Failed,
}

impl StartFlow {
    fn is_started(&self) -> bool {
        matches!(self, StartFlow::Started)
    }
}

/// Run one authentication conversation to its terminal response. The [`Login`]
/// pumps prompts and replies over its own clone of the connection; we only send
/// the final verdict here, padding the failure path to [`MIN_AUTH_FAILURE`] so a
/// fast rejection can't be timed. On success the login is now bound to the user,
/// which the `Start` gate reads — the loop keeps serving either way.
fn run_auth(
    conn: &mut UnixStream,
    login: &mut dyn Login,
    username: &str,
) -> Result<AuthFlow, FrameError> {
    let started = Instant::now();
    let outcome = login.authenticate(username);

    match outcome {
        AuthOutcome::Success => {
            eprintln!("doord: authentication succeeded for '{username}'");
            write_frame(conn, &Response::AuthSuccess)?;
            Ok(AuthFlow::Continue)
        }
        AuthOutcome::Failure => {
            pad_failure(started);
            write_frame(
                conn,
                &Response::AuthFailure {
                    reason: "Authentication failed".to_string(),
                },
            )?;
            Ok(AuthFlow::Continue)
        }
        AuthOutcome::Cancelled => {
            write_frame(
                conn,
                &Response::AuthFailure {
                    reason: "Authentication cancelled".to_string(),
                },
            )?;
            Ok(AuthFlow::Continue)
        }
        AuthOutcome::Transport => {
            // The greeter is gone; nothing left to reply to.
            eprintln!("doord: greeter vanished mid-authentication");
            Ok(AuthFlow::Break)
        }
    }
}

/// Launch the chosen session for the already-authenticated login, performing the
/// greeter→session VT handoff (D-0008). Resolves the session id against current
/// discovery; if doord manages the greeter, it **tears the greeter down and waits
/// for the VT to free before** the session takes it; then it hands off to the
/// [`Login`] (which opens the logind session, drops privilege, execs) and owns the
/// running session until it exits. An unknown session is refused without touching
/// the greeter (the greeter keeps serving so the user can choose again).
fn run_start(
    conn: &mut UnixStream,
    config: &Config,
    login: &mut dyn Login,
    managed_greeter: Option<&RefCell<GreeterHandle>>,
    session_id: &str,
) -> Result<StartFlow, FrameError> {
    // For journaling only; the launch binds to the login's authenticated user,
    // not to anything derived from the request.
    let username = login.user().unwrap_or("?").to_string();

    // Re-discover so the picker and the launch agree on the same id, even if the
    // installed set changed while the greeter was up. The Exec stays daemon-side.
    let session = crate::sessions::discover(&config.session_dirs)
        .into_iter()
        .find(|s| s.id == session_id);

    let session = match session {
        Some(session) => session,
        None => {
            // Greeter is untouched — it keeps serving so the user can choose again.
            eprintln!("doord: refusing Start: no session with id '{session_id}'");
            write_frame(
                conn,
                &Response::Error {
                    message: "no such session".to_string(),
                },
            )?;
            return Ok(StartFlow::Failed);
        }
    };

    if let Some(greeter) = managed_greeter {
        // Managed handoff: free the VT before the session takes it. Tearing the
        // greeter down also drops this connection (we are serving the greeter) —
        // that is intended; its job is done. We therefore send the greeter
        // nothing and drive the session over the login's control channel.
        eprintln!(
            "doord: handoff '{username}' → session '{}': tearing down greeter, freeing the VT",
            session.id
        );
        greeter.borrow_mut().terminate();

        match login.start(&session, &config.seat) {
            Ok(child) => {
                eprintln!("doord: started session '{}' for '{username}'", session.id);
                match child.wait() {
                    Ok(Some(status)) => {
                        eprintln!("doord: session '{}' exited ({status})", session.id)
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("doord: waiting on session '{}' failed: {e}", session.id),
                }
            }
            Err(e) => {
                // The greeter is already gone; the loop re-greets after we return.
                eprintln!("doord: session start failed after handoff for '{username}': {e}");
            }
        }
        // The connection (greeter) is finished either way; the loop re-greets.
        return Ok(StartFlow::Started);
    }

    // Unmanaged (dev): no greeter to tear down. Tell the greeter it started, then
    // own the session; a failure keeps the connection serving.
    match login.start(&session, &config.seat) {
        Ok(child) => {
            eprintln!("doord: started session '{}' for '{username}'", session.id);
            write_frame(conn, &Response::Started)?;
            match child.wait() {
                Ok(Some(status)) => {
                    eprintln!("doord: session '{}' exited ({status})", session.id)
                }
                Ok(None) => {}
                Err(e) => eprintln!("doord: waiting on session '{}' failed: {e}", session.id),
            }
            Ok(StartFlow::Started)
        }
        Err(e) => {
            eprintln!("doord: could not start session '{}' for '{username}': {e}", session.id);
            write_frame(
                conn,
                &Response::Error {
                    message: "could not start the session".to_string(),
                },
            )?;
            Ok(StartFlow::Failed)
        }
    }
}

/// Sleep until at least [`MIN_AUTH_FAILURE`] has elapsed since `started`.
fn pad_failure(started: Instant) {
    let elapsed = started.elapsed();
    if elapsed < MIN_AUTH_FAILURE {
        thread::sleep(MIN_AUTH_FAILURE - elapsed);
    }
}

/// The mandatory first exchange: the greeter must send [`Request::Hello`] and
/// declare a version this daemon can speak. Anything else, or an incompatible
/// version, ends the connection. Returns `true` if the handshake succeeded.
fn handshake(conn: &mut UnixStream) -> Result<bool, FrameError> {
    let first: Request = read_frame(conn)?;
    match first {
        Request::Hello { protocol_version } if protocol_version == PROTOCOL_VERSION => {
            write_frame(
                conn,
                &Response::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                },
            )?;
            Ok(true)
        }
        Request::Hello { protocol_version } => {
            eprintln!(
                "doord: rejecting greeter speaking protocol v{protocol_version}; this daemon speaks v{PROTOCOL_VERSION}"
            );
            write_frame(
                conn,
                &Response::Incompatible {
                    daemon_protocol_version: PROTOCOL_VERSION,
                },
            )?;
            Ok(false)
        }
        _ => {
            // A credential or any other request before Hello is a protocol
            // violation; refuse without acting on it.
            eprintln!("doord: greeter sent a request before the handshake; closing");
            write_frame(
                conn,
                &Response::Error {
                    message: "handshake required before any other request".to_string(),
                },
            )?;
            Ok(false)
        }
    }
}

/// Turn a (post-handshake) request into a response. The remaining privileged
/// actions (spawn, power) are not implemented yet; until they land, every
/// request that would touch them gets a clear, non-leaky refusal.
fn dispatch(request: &Request, config: &Config) -> Response {
    match request {
        // A second Hello is harmless; re-acknowledge.
        Request::Hello { .. } => Response::Welcome {
            protocol_version: PROTOCOL_VERSION,
        },
        Request::ListSessions => {
            // Re-scanned per request so a session installed while the greeter is
            // up appears without restarting the daemon. Only the greeter-facing
            // projection crosses the seam — the `Exec` stays in the daemon.
            let sessions = crate::sessions::discover(&config.session_dirs)
                .iter()
                .map(crate::sessions::DiscoveredSession::to_wire)
                .collect();
            Response::Sessions(sessions)
        }
        // BeginAuth and Start are handled by the stateful connection loop, not
        // here; reaching this arm is an internal routing bug.
        Request::BeginAuth { .. } | Request::Start { .. } => Response::Error {
            message: "internal: stateful request reached the stateless dispatch".to_string(),
        },
        // A reply or cancel with no conversation in progress is a stray message.
        Request::AuthReply { .. } | Request::CancelAuth => Response::Error {
            message: "no authentication in progress".to_string(),
        },
        Request::Power(_) => Response::Error {
            message: "not yet available: the privileged core is still being built".to_string(),
        },
    }
}

/// Read the peer's kernel-attested credentials via `SO_PEERCRED`. The uid here
/// is asserted by the kernel, not by anything the greeter sent, which is what
/// makes it a trustworthy authorization signal.
fn peer_cred(stream: &UnixStream) -> io::Result<libc::ucred> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `len` bytes into `cred`, which is a
    // correctly sized `ucred`; `len` is updated in place to the bytes written.
    let ret = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(cred)
}

/// `chgrp` the socket to `gid`, keeping the owner unchanged (`uid == -1`).
fn chown_group(path: &Path, gid: u32) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "socket path has a NUL byte"))?;
    // SAFETY: c_path is a valid NUL-terminated C string for the call's duration.
    let ret = unsafe { libc::chown(c_path.as_ptr(), u32::MAX, gid) };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SeatTarget;
    use crate::pam::testing::ScriptedLoginFactory;
    use protocol::{AuthPrompt, Secret};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    fn test_config() -> Config {
        Config {
            socket_path: PathBuf::from("/unused-in-pair-test.sock"),
            session_dirs: Vec::new(),
            // A socketpair reports the creating process's creds on SO_PEERCRED,
            // so authorize our own uid.
            greeter_uid: unsafe { libc::getuid() },
            greeter_gid: None,
            pam_service: "unused-in-pair-test".to_string(),
            seat: SeatTarget {
                seat: "seat0".to_string(),
                vtnr: None,
            },
            // Tests drive the connection directly; no managed greeter.
            greeter_user: None,
            greeter_pam_service: "unused".to_string(),
            greeter_cmd: vec!["true".to_string()],
        }
    }

    /// A scripted login factory accepting `hunter2`, recording launches into the
    /// shared log the test thread reads — so the connection's auth-gating and
    /// identity-binding can be asserted without privilege, PAM, or a real spawn.
    fn scripted_logins(calls: Arc<Mutex<Vec<(String, String)>>>) -> ScriptedLoginFactory {
        ScriptedLoginFactory {
            password: "hunter2".to_string(),
            calls,
        }
    }

    static SESSION_ROOT_COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Create a throwaway data-dir root holding one session with the given id,
    /// returned as a one-element `session_dirs`. Leaked (not cleaned) — these are
    /// tiny and live under the temp dir; keeping the helper trivial matters more.
    fn session_dir_with(id: &str) -> Vec<PathBuf> {
        let n = SESSION_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "doord-ipc-sesstest-{}-{}",
            std::process::id(),
            n
        ));
        let wayland = root.join("wayland-sessions");
        std::fs::create_dir_all(&wayland).unwrap();
        std::fs::write(
            wayland.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nName={id}\nExec=/usr/bin/{id}\n"),
        )
        .unwrap();
        vec![root]
    }

    /// Drive a full handshake + auth conversation over an in-process socketpair
    /// against the scripted authenticator, returning the terminal response to
    /// the supplied password.
    fn run_conversation(password_typed: &str) -> Response {
        let (mut client, server) = UnixStream::pair().unwrap();
        let typed = password_typed.to_string();
        let handle = thread::spawn(move || {
            let cfg = test_config();
            let logins = scripted_logins(Arc::new(Mutex::new(Vec::new())));
            let _ = handle_connection(server, &cfg, &logins, None);
            let _ = typed; // captured to keep the closure's intent explicit
        });

        write_frame(
            &mut client,
            &Request::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .unwrap();
        assert_eq!(
            read_frame::<_, Response>(&mut client).unwrap(),
            Response::Welcome {
                protocol_version: PROTOCOL_VERSION
            }
        );

        write_frame(
            &mut client,
            &Request::BeginAuth {
                username: "stephen".to_string(),
            },
        )
        .unwrap();
        // The conversation's first step is the password prompt.
        assert_eq!(
            read_frame::<_, Response>(&mut client).unwrap(),
            Response::Auth(AuthPrompt::Question {
                text: "Password:".to_string(),
                secret: true,
            })
        );

        write_frame(
            &mut client,
            &Request::AuthReply {
                response: Secret::new(password_typed.to_string()),
            },
        )
        .unwrap();
        let terminal: Response = read_frame(&mut client).unwrap();

        drop(client);
        handle.join().unwrap();
        terminal
    }

    #[test]
    fn correct_password_authenticates() {
        assert_eq!(run_conversation("hunter2"), Response::AuthSuccess);
    }

    #[test]
    fn wrong_password_is_refused() {
        assert!(matches!(
            run_conversation("wrong"),
            Response::AuthFailure { .. }
        ));
    }

    #[test]
    fn cancel_during_conversation_ends_it() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let handle = thread::spawn(move || {
            let cfg = test_config();
            let logins = scripted_logins(Arc::new(Mutex::new(Vec::new())));
            let _ = handle_connection(server, &cfg, &logins, None);
        });

        write_frame(
            &mut client,
            &Request::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .unwrap();
        let _welcome: Response = read_frame(&mut client).unwrap();

        write_frame(
            &mut client,
            &Request::BeginAuth {
                username: "stephen".to_string(),
            },
        )
        .unwrap();
        let _prompt: Response = read_frame(&mut client).unwrap();

        // Cancel instead of answering.
        write_frame(&mut client, &Request::CancelAuth).unwrap();
        let terminal: Response = read_frame(&mut client).unwrap();
        assert!(matches!(terminal, Response::AuthFailure { .. }));

        drop(client);
        handle.join().unwrap();
    }

    #[test]
    fn start_before_auth_is_refused_and_never_launches() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let cfg = Config {
                session_dirs: session_dir_with("hyprland"),
                ..test_config()
            };
            let logins = scripted_logins(calls_in);
            let _ = handle_connection(server, &cfg, &logins, None);
        });

        // Handshake, then jump straight to Start without authenticating.
        write_frame(
            &mut client,
            &Request::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .unwrap();
        let _welcome: Response = read_frame(&mut client).unwrap();

        write_frame(
            &mut client,
            &Request::Start {
                session_id: "hyprland".to_string(),
            },
        )
        .unwrap();
        let resp: Response = read_frame(&mut client).unwrap();
        assert!(
            matches!(resp, Response::Error { .. }),
            "Start without a prior auth must be refused, got {resp:?}"
        );

        drop(client);
        handle.join().unwrap();

        // The privilege handoff was never reached.
        assert!(
            calls.lock().unwrap().is_empty(),
            "no session may be launched without authentication"
        );
    }

    #[test]
    fn start_after_auth_launches_the_chosen_session_as_the_authed_user() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let cfg = Config {
                session_dirs: session_dir_with("hyprland"),
                ..test_config()
            };
            let logins = scripted_logins(calls_in);
            let _ = handle_connection(server, &cfg, &logins, None);
        });

        write_frame(
            &mut client,
            &Request::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .unwrap();
        let _welcome: Response = read_frame(&mut client).unwrap();

        // Authenticate as "stephen".
        write_frame(
            &mut client,
            &Request::BeginAuth {
                username: "stephen".to_string(),
            },
        )
        .unwrap();
        let _prompt: Response = read_frame(&mut client).unwrap();
        write_frame(
            &mut client,
            &Request::AuthReply {
                response: Secret::new("hunter2".to_string()),
            },
        )
        .unwrap();
        assert_eq!(
            read_frame::<_, Response>(&mut client).unwrap(),
            Response::AuthSuccess
        );

        // Now Start succeeds and the daemon reports the session launched.
        write_frame(
            &mut client,
            &Request::Start {
                session_id: "hyprland".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            read_frame::<_, Response>(&mut client).unwrap(),
            Response::Started
        );

        drop(client);
        handle.join().unwrap();

        // Exactly the chosen session was launched, bound to the authenticated
        // user — not anything the greeter could otherwise have named.
        assert_eq!(
            *calls.lock().unwrap(),
            vec![("hyprland".to_string(), "stephen".to_string())]
        );
    }
}
