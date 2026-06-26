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
use crate::pam::{AuthOutcome, Authenticator, SocketChannel};

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
pub fn serve(config: &Config, authenticator: &dyn Authenticator) -> io::Result<()> {
    let listener = bind(config)?;
    eprintln!(
        "doord: listening on {} (greeter uid {}, pam service '{}')",
        config.socket_path.display(),
        config.greeter_uid,
        config.pam_service
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                // Sequential by construction: we fully serve one greeter before
                // accepting the next. One seat, one greeter (no concurrency).
                if let Err(e) = handle_connection(stream, config, authenticator) {
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
    authenticator: &dyn Authenticator,
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

        // BeginAuth opens a multi-step PAM conversation that does its own socket
        // round-trips through the connection; the rest are single request →
        // response.
        match request {
            Request::BeginAuth { username } => {
                if run_auth(&mut conn, authenticator, &username)?.is_break() {
                    return Ok(());
                }
            }
            other => {
                let response = dispatch(&other);
                write_frame(&mut conn, &response)?;
            }
        }
    }
}

/// Whether the connection should keep serving or be torn down.
enum AuthFlow {
    Continue,
    Break,
}

impl AuthFlow {
    fn is_break(&self) -> bool {
        matches!(self, AuthFlow::Break)
    }
}

/// Run one authentication conversation to its terminal response. The
/// [`Authenticator`] pumps prompts and replies over `conn` via a
/// [`SocketChannel`]; we only send the final verdict here, padding the failure
/// path to [`MIN_AUTH_FAILURE`] so a fast rejection can't be timed.
fn run_auth(
    conn: &mut UnixStream,
    authenticator: &dyn Authenticator,
    username: &str,
) -> Result<AuthFlow, FrameError> {
    let started = Instant::now();
    let outcome = {
        let mut channel = SocketChannel::new(conn);
        authenticator.authenticate(username, &mut channel)
    };

    let response = match outcome {
        AuthOutcome::Success => {
            eprintln!("doord: authentication succeeded for '{username}'");
            Response::AuthSuccess
        }
        AuthOutcome::Failure => {
            pad_failure(started);
            Response::AuthFailure {
                reason: "Authentication failed".to_string(),
            }
        }
        AuthOutcome::Cancelled => Response::AuthFailure {
            reason: "Authentication cancelled".to_string(),
        },
        AuthOutcome::Transport => {
            // The greeter is gone; nothing left to reply to.
            eprintln!("doord: greeter vanished mid-authentication");
            return Ok(AuthFlow::Break);
        }
    };

    write_frame(conn, &response)?;
    Ok(AuthFlow::Continue)
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

/// Turn a (post-handshake) request into a response. The privileged actions are
/// not implemented yet; until session discovery, PAM, and spawn land, every
/// request that would touch them gets a clear, non-leaky refusal.
fn dispatch(request: &Request) -> Response {
    match request {
        // A second Hello is harmless; re-acknowledge.
        Request::Hello { .. } => Response::Welcome {
            protocol_version: PROTOCOL_VERSION,
        },
        Request::ListSessions => Response::Sessions(Vec::new()),
        // BeginAuth is handled by the conversation path, not here.
        Request::BeginAuth { .. } => Response::Error {
            message: "internal: auth request reached the stateless dispatch".to_string(),
        },
        // A reply or cancel with no conversation in progress is a stray message.
        Request::AuthReply { .. } | Request::CancelAuth => Response::Error {
            message: "no authentication in progress".to_string(),
        },
        Request::Start { .. } | Request::Power(_) => Response::Error {
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
    use crate::pam::ScriptedAuthenticator;
    use protocol::{AuthPrompt, Secret};
    use std::path::PathBuf;

    fn test_config() -> Config {
        Config {
            socket_path: PathBuf::from("/unused-in-pair-test.sock"),
            // A socketpair reports the creating process's creds on SO_PEERCRED,
            // so authorize our own uid.
            greeter_uid: unsafe { libc::getuid() },
            greeter_gid: None,
            pam_service: "unused-in-pair-test".to_string(),
        }
    }

    /// Drive a full handshake + auth conversation over an in-process socketpair
    /// against the scripted authenticator, returning the terminal response to
    /// the supplied password.
    fn run_conversation(password_typed: &str) -> Response {
        let (mut client, server) = UnixStream::pair().unwrap();
        let typed = password_typed.to_string();
        let handle = thread::spawn(move || {
            let cfg = test_config();
            let auth = ScriptedAuthenticator {
                password: "hunter2".to_string(),
            };
            let _ = handle_connection(server, &cfg, &auth);
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
            let auth = ScriptedAuthenticator {
                password: "hunter2".to_string(),
            };
            let _ = handle_connection(server, &cfg, &auth);
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
}
