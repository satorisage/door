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
//! Session discovery, the PAM conversation, and session spawn are not built yet;
//! until they are, post-handshake requests get a structured "not yet available"
//! refusal. The seam, its framing, and its authorization are real and
//! exercisable today.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use protocol::{read_frame, write_frame, FrameError, Request, Response, PROTOCOL_VERSION};

use crate::config::Config;
use crate::pam::{AuthOutcome, Login, LoginFactory};

/// How long the daemon waits on a stalled read before dropping the peer.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Floor on how long a *failed* authentication takes to report. A wrong username
/// must not fail visibly faster than a wrong password, or the timing itself
/// leaks which accounts exist. Only the failure path is padded — success is not
/// slowed. Lockout/backoff proper is left to the PAM stack (`pam_faillock`).
const MIN_AUTH_FAILURE: Duration = Duration::from_secs(1);

/// Bind the socket and serve greeters forever. Returns only on a fatal error
/// setting up the listener; per-connection errors are logged and the loop
/// continues.
pub fn serve(config: &Config, logins: &dyn LoginFactory) -> io::Result<()> {
    let listener = bind(config)?;
    eprintln!(
        "doord: listening on {} (greeter uid {}, pam service '{}', seat '{}' vt {:?})",
        config.socket_path.display(),
        config.greeter_uid,
        config.pam_service,
        config.seat.seat,
        config.seat.vtnr,
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                // Sequential by construction: we fully serve one greeter before
                // accepting the next. One seat, one greeter (no concurrency).
                if let Err(e) = handle_connection(stream, config, logins) {
                    eprintln!("doord: connection ended: {e}");
                }
            }
            Err(e) => eprintln!("doord: accept failed: {e}"),
        }
    }
    Ok(())
}

/// Create the socket directory, remove any stale socket, bind, and lock down
/// permissions and ownership.
fn bind(config: &Config) -> io::Result<UnixListener> {
    let path = &config.socket_path;

    if let Some(dir) = path.parent() {
        // The directory gates who can even see the socket: root-owned, 0700. We
        // only lock down a directory we create — if it already exists we may not
        // own it (e.g. a dev socket placed directly under /tmp), and chmod'ing a
        // shared dir would both fail and be wrong. In production the dedicated
        // /run/doord is created here (or by the unit's RuntimeDirectory) fresh.
        if !dir.exists() {
            fs::create_dir_all(dir)?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
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
                } else if run_start(&mut conn, config, login.as_mut(), &session_id)?.is_started() {
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

/// Launch the chosen session for the already-authenticated login. Resolves the
/// session id against current discovery, hands off to the [`Login`] (which opens
/// the logind session, drops privilege, and execs), tells the greeter
/// [`Response::Started`], then owns the running session until it exits. A logical
/// failure (unknown session, spawn error) sends a non-leaky [`Response::Error`]
/// and keeps the connection serving so the greeter can choose again.
fn run_start(
    conn: &mut UnixStream,
    config: &Config,
    login: &mut dyn Login,
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

    match login.start(&session, &config.seat) {
        Ok(child) => {
            eprintln!("doord: started session '{}' for '{username}'", session.id);
            // Tell the greeter before we block on the session: it steps aside,
            // the daemon stays as the session's parent for its whole lifetime.
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
            // Detail to the journal; the login screen sees only a generic refusal
            // (a spawn error must not, e.g., reveal whether an account exists).
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
            let _ = handle_connection(server, &cfg, &logins);
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
            let _ = handle_connection(server, &cfg, &logins);
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
            let _ = handle_connection(server, &cfg, &logins);
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
            let _ = handle_connection(server, &cfg, &logins);
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
