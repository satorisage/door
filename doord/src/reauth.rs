//! The session-lock **reauth** verb: a verify-only privileged path that
//! reauthenticates the *connecting peer's own uid* via PAM and returns allow/deny.
//!
//! This is the daemon side of the session lock screen. The future `door-lock`
//! (an `ext-session-lock-v1` client, a separate task) renders the locked surface as
//! the logged-in session user and asks doord — over this seam — to verify that user's
//! credentials so it can unlock. Unlike the greeter login path, reauth **never opens a
//! PAM session, never becomes a logind leader, never takes a VT, never drops privilege
//! to exec, and never spawns a session**. It verifies and reports — nothing else.
//!
//! Two properties are load-bearing and enforced structurally here:
//!
//! - **uid binding.** The reauth target identity is the peer's kernel-attested
//!   `SO_PEERCRED` uid, resolved to a username via [`crate::user::resolve_uid`] — never
//!   a name the client sent (the wire vocabulary has no username field). A client of
//!   uid X can never make the daemon verify uid Y's credentials, so this seam is not a
//!   cross-user brute-force oracle.
//! - **no session, ever.** The PAM engine is reused verbatim (the same re-exec'd
//!   session worker, the same multi-round FIDO2/multi-prompt conversation), but the
//!   relay sends the worker only [`WorkerCommand::Auth`] — never
//!   [`WorkerCommand::Start`]. `Start` is the only thing that opens a session, and the
//!   reauth relay has no code path that emits it; when the attempt finishes the
//!   worker's control socket is closed and the worker exits having only run
//!   `pam_authenticate` + `pam_acct_mgmt`.
//!
//! Why the PAM still runs in a worker (not here in the listener): the supervisor runs
//! under a seccomp allowlist with no `clone`/`execve` and an optional Landlock
//! ruleset. Running PAM (esp. FIDO2/libfido2) in the confined supervisor would be
//! blocked or force widening its sandbox. Workers are forked off a pre-sandbox
//! spawner, outside that confinement — exactly as the login path does. The same
//! no-`clone` reality is why the listener is a single thread (spawned before the
//! sandbox) serving connections sequentially rather than a thread per connection.

use std::fs;
use std::io;
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use protocol::{
    read_frame, write_frame, AuthPrompt, FrameError, ReauthRequest, ReauthResponse,
    PROTOCOL_VERSION,
};

use crate::config::Config;
use crate::ipc::{pad_failure, peer_cred};
use crate::spawner::Spawner;
use crate::worker::{Verdict, WorkerCommand, WorkerEvent};
use crate::{pam, user};

/// Bound on the machine-paced reads (the handshake and each inter-attempt request).
/// A real lock client sends these promptly; a peer that connects and stalls before an
/// attempt begins is dropped after this, so it cannot pin the single-threaded listener
/// forever. This seam is world-reachable (any local uid may connect), so unlike the
/// greeter's unbounded idle wait, every read here is bounded.
const REAUTH_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Bound on a *human* reply during an active PAM conversation (typing a password,
/// touching a FIDO2 key). Generous, because a person is on the other end; still finite,
/// so a stalled conversation is eventually reclaimed.
const REAUTH_REPLY_TIMEOUT: Duration = Duration::from_secs(120);

/// The outcome of one reauth attempt, as the listener will report it. The verified
/// identity is always the peer-cred uid — never anything carried here.
#[derive(Debug, PartialEq, Eq)]
pub enum ReauthOutcome {
    /// The peer's own credentials verified (`pam_authenticate` + `pam_acct_mgmt`).
    Allow,
    /// Credentials rejected, the account check denied, or a daemon-side fault —
    /// reported coarsely and padded, so a fast rejection cannot be timed and nothing
    /// reveals whether the account exists. Fails closed (a fault denies).
    Deny,
    /// The user asked to cancel the in-progress conversation.
    Cancelled,
    /// The *client* connection broke mid-conversation; there is nothing to reply to.
    Transport,
}

impl From<Verdict> for ReauthOutcome {
    fn from(v: Verdict) -> Self {
        match v {
            Verdict::Success => ReauthOutcome::Allow,
            Verdict::Failure => ReauthOutcome::Deny,
            Verdict::Cancelled => ReauthOutcome::Cancelled,
            // A transport verdict from the worker means the daemon↔worker channel
            // broke — since we (the daemon) are the one reading it, that is a
            // daemon-side fault, so fail closed rather than treat it as client-gone.
            Verdict::Transport => ReauthOutcome::Deny,
        }
    }
}

/// One verify-only reauthentication. Created per attempt; relays a PAM conversation to
/// the lock client and returns the verdict. It has **no** method that opens a session
/// or spawns anything — "verify only" is a property of this trait's surface, not of a
/// caller that declines.
pub(crate) trait Reauthenticator {
    /// Verify `username`'s credentials via a fresh PAM auth conversation relayed to
    /// `client`. `username` is always the peer-cred-derived identity.
    fn reauthenticate(&mut self, username: &str, client: &mut UnixStream) -> ReauthOutcome;
}

/// Builds a fresh [`Reauthenticator`] per attempt. Abstracted so the listener/handler
/// can be tested in-process without root, a live PAM stack, or a real worker (mirrors
/// the login path's [`crate::pam::LoginFactory`]).
pub(crate) trait ReauthFactory {
    fn begin(&self) -> Box<dyn Reauthenticator>;
}

/// Production [`ReauthFactory`]. Each attempt gets a session worker running a
/// verify-only PAM conversation. With a **spawner** present (production), the worker is
/// forked by a pre-sandbox spawner, outside the supervisor's confinement; without one
/// (dev) it is forked directly. The spawner is shared across the listener's sequential
/// attempts behind a mutex (locked only during the brief spawn request).
pub(crate) struct WorkerReauthFactory {
    spawner: Arc<Mutex<Option<Spawner>>>,
}

impl WorkerReauthFactory {
    /// Wrap the reauth subsystem's own spawner handle (`None` selects the direct
    /// dev/non-spawner fork path).
    pub fn new(spawner: Arc<Mutex<Option<Spawner>>>) -> Self {
        WorkerReauthFactory { spawner }
    }
}

impl ReauthFactory for WorkerReauthFactory {
    fn begin(&self) -> Box<dyn Reauthenticator> {
        let guard = self.spawner.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            // Split path: ask the reauth spawner for a worker; it hands back the pid
            // (it reaps) and the control socket (we relay over it, then close it).
            Some(spawner) => match spawner.request_worker() {
                Ok(child) => Box::new(WorkerReauth {
                    control: child.control,
                    owner: WorkerOwner::Spawned,
                }),
                Err(e) => {
                    eprintln!("doord: reauth: spawner could not start a worker: {e}");
                    Box::new(DeadReauth)
                }
            },
            // Direct path: fork the worker here (dev/non-spawner; no sandbox in play).
            None => match pam::spawn_worker() {
                Ok((child, control)) => Box::new(WorkerReauth {
                    control,
                    owner: WorkerOwner::Direct(Some(child)),
                }),
                Err(e) => {
                    eprintln!("doord: reauth: could not start a worker: {e}");
                    Box::new(DeadReauth)
                }
            },
        }
    }
}

/// How this attempt's worker is owned — which decides how it is reaped.
enum WorkerOwner {
    /// Direct path: the worker is this process's child; wait on it to reap.
    Direct(Option<Child>),
    /// Split path: the worker is the reauth spawner's child; it reaps. We only close
    /// the control socket, whose EOF tells the worker to exit.
    Spawned,
}

/// A worker-backed reauthentication: the daemon's end of the worker control socket and
/// how the worker is owned. On drop it closes the control channel — which, because the
/// worker only ever received an `Auth` command (never `Start`), tells the worker to
/// exit **without having opened a session**.
struct WorkerReauth {
    control: UnixStream,
    owner: WorkerOwner,
}

impl Reauthenticator for WorkerReauth {
    fn reauthenticate(&mut self, username: &str, client: &mut UnixStream) -> ReauthOutcome {
        // The one and only command this path ever sends the worker. There is no
        // `Start` anywhere below, so the worker runs `pam_authenticate` +
        // `pam_acct_mgmt` and never `pam_open_session`.
        if write_frame(
            &mut self.control,
            &WorkerCommand::Auth {
                username: username.to_string(),
            },
        )
        .is_err()
        {
            eprintln!("doord: reauth: worker vanished before auth; denying");
            return ReauthOutcome::Deny;
        }

        loop {
            let event = match read_frame::<_, WorkerEvent>(&mut self.control) {
                Ok(event) => event,
                Err(_) => {
                    eprintln!("doord: reauth: worker vanished mid-conversation; denying");
                    return ReauthOutcome::Deny;
                }
            };

            match event {
                WorkerEvent::Prompt { text, secret } => {
                    let prompt = ReauthResponse::Prompt(AuthPrompt::Question { text, secret });
                    if write_frame(client, &prompt).is_err() {
                        let _ = write_frame(&mut self.control, &WorkerCommand::Cancel);
                        return ReauthOutcome::Transport;
                    }
                    match read_frame::<_, ReauthRequest>(client) {
                        Ok(ReauthRequest::Reply { response }) => {
                            if write_frame(&mut self.control, &WorkerCommand::Reply { response })
                                .is_err()
                            {
                                return ReauthOutcome::Deny;
                            }
                        }
                        // Cancel: tell the worker to abort; keep looping to collect the
                        // terminal `Cancelled` verdict it will emit.
                        Ok(ReauthRequest::Cancel) => {
                            let _ = write_frame(&mut self.control, &WorkerCommand::Cancel);
                        }
                        // Any other frame mid-conversation is a client protocol error:
                        // abort the worker and drop the client.
                        Ok(_) => {
                            let _ = write_frame(&mut self.control, &WorkerCommand::Cancel);
                            return ReauthOutcome::Transport;
                        }
                        // The client stalled past the reply timeout or disconnected.
                        Err(_) => {
                            let _ = write_frame(&mut self.control, &WorkerCommand::Cancel);
                            return ReauthOutcome::Transport;
                        }
                    }
                }
                WorkerEvent::Info { text } => {
                    let _ = write_frame(client, &ReauthResponse::Prompt(AuthPrompt::Info { text }));
                }
                WorkerEvent::Error { text } => {
                    let _ =
                        write_frame(client, &ReauthResponse::Prompt(AuthPrompt::Error { text }));
                }
                WorkerEvent::Auth(verdict) => return ReauthOutcome::from(verdict),
                // A session event is impossible on this path — reauth never sends
                // `Start`. If one somehow arrives, deny (never treat it as success).
                WorkerEvent::Started | WorkerEvent::StartFailed => {
                    eprintln!("doord: reauth: unexpected session event from worker; denying");
                    return ReauthOutcome::Deny;
                }
            }
        }
    }
}

impl Drop for WorkerReauth {
    fn drop(&mut self) {
        // Close the control channel first so the worker (blocked reading its next
        // command) sees EOF and exits; it only ever ran auth, so it opened no session.
        let _ = self.control.shutdown(Shutdown::Both);
        if let WorkerOwner::Direct(child) = &mut self.owner {
            if let Some(mut c) = child.take() {
                let _ = c.wait();
            }
        }
    }
}

/// A reauthentication whose worker could not be started: fails closed, so a
/// worker-spawn failure denies (stays locked) rather than ever allowing.
struct DeadReauth;

impl Reauthenticator for DeadReauth {
    fn reauthenticate(&mut self, _username: &str, _client: &mut UnixStream) -> ReauthOutcome {
        ReauthOutcome::Deny
    }
}

/// The reauth listener. Binds the reauth socket and serves verify-only reauth
/// attempts, one connection at a time, for the daemon's lifetime. Called on its own
/// thread from `main` (spawned before the sandbox, since the supervisor's seccomp
/// allowlist has no `clone`). Returns only on a fatal error binding the listener;
/// per-connection errors are logged and serving continues.
pub(crate) fn serve_reauth(config: &Config, factory: &dyn ReauthFactory) -> io::Result<()> {
    let listener = bind_reauth(config)?;
    eprintln!(
        "doord: reauth listener on {} (any local uid may connect; the peer-cred uid is the identity)",
        config.reauth_socket_path.display()
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle_reauth_connection(stream, factory) {
                    eprintln!("doord: reauth connection ended: {e}");
                }
            }
            Err(e) => eprintln!("doord: reauth accept failed: {e}"),
        }
    }
    Ok(())
}

/// Ensure the reauth socket's **dedicated** directory exists and is world-traversable
/// (`0755`) — any local session user must reach the socket to unlock their own session;
/// the peer-cred uid, not the dir mode, is the authorization gate. Idempotent.
///
/// Called twice, deliberately: once on the main thread **before the sandbox**, so the
/// dir exists when the Landlock ruleset seeds it as a `PathFd` (a missing dir would make
/// the whole ruleset install fail, silently un-sandboxing the supervisor); and again
/// from [`bind_reauth`] on the listener thread. The `0755` is set unconditionally so a
/// systemd `RuntimeDirectory` that pre-created the dir at `0700` is widened to
/// traversable. `DOORD_REAUTH_SOCKET` is expected to name a path inside a dedicated dir
/// (the default `/run/doord-reauth/…` and the tests' per-run temp subdir both do), so
/// this only ever chmods a directory the daemon owns — never a shared parent like `/tmp`.
pub(crate) fn ensure_socket_dir(config: &Config) -> io::Result<()> {
    if let Some(dir) = config.reauth_socket_path.parent() {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Ensure the socket directory, remove any stale socket, bind, and make the socket
/// world-connectable. The peer-cred uid, not the socket mode, is the authorization
/// gate on this seam.
fn bind_reauth(config: &Config) -> io::Result<UnixListener> {
    let path = &config.reauth_socket_path;
    ensure_socket_dir(config)?;

    if path.exists() {
        fs::remove_file(path)?;
    }

    let listener = UnixListener::bind(path)?;
    // 0666: any local uid may connect. Each connection can only ever reauthenticate as
    // its own peer-cred uid, so world-connect is intended, not a weakness.
    fs::set_permissions(path, fs::Permissions::from_mode(0o666))?;
    Ok(listener)
}

/// Serve one reauth client: bind the target identity to its peer-cred uid, run the
/// version handshake, then serve verify-only reauth attempts until it disconnects.
fn handle_reauth_connection(
    stream: UnixStream,
    factory: &dyn ReauthFactory,
) -> Result<(), FrameError> {
    let cred = peer_cred(&stream)?;
    // The one place the reauth identity is decided: the kernel-attested peer uid,
    // resolved to a username here. Nothing the client sends contributes to it — the
    // wire vocabulary has no username field. A uid with no passwd entry yields `None`,
    // and every attempt on it is denied coarsely (never revealing it is unknown).
    let username = user::resolve_uid(cred.uid).ok().map(|u| u.name);

    let mut conn = stream;
    conn.set_read_timeout(Some(REAUTH_HANDSHAKE_TIMEOUT))?;

    if !reauth_handshake(&mut conn)? {
        return Ok(());
    }

    loop {
        // Machine-paced bound between attempts; a peer that goes silent here is dropped.
        conn.set_read_timeout(Some(REAUTH_HANDSHAKE_TIMEOUT))?;
        let request: ReauthRequest = match read_frame(&mut conn) {
            Ok(req) => req,
            Err(e) if is_client_gone(&e) => return Ok(()),
            Err(e) => {
                // Never echo the offending bytes (they may be a mistyped secret).
                eprintln!("doord: reauth: dropping client after frame error: {e}");
                let _ = write_frame(
                    &mut conn,
                    &ReauthResponse::Error {
                        message: "malformed request".to_string(),
                    },
                );
                return Ok(());
            }
        };

        match request {
            ReauthRequest::Begin => {
                let started = Instant::now();
                let outcome = match &username {
                    Some(name) => {
                        // A human is about to be prompted; widen the reply window.
                        conn.set_read_timeout(Some(REAUTH_REPLY_TIMEOUT))?;
                        let mut auth = factory.begin();
                        let outcome = auth.reauthenticate(name, &mut conn);
                        conn.set_read_timeout(Some(REAUTH_HANDSHAKE_TIMEOUT))?;
                        outcome
                    }
                    // No passwd entry for the peer uid: cannot reauthenticate. Deny
                    // coarsely, padded, without ever revealing the account is unknown.
                    None => ReauthOutcome::Deny,
                };

                match outcome {
                    ReauthOutcome::Allow => write_frame(&mut conn, &ReauthResponse::Allow)?,
                    ReauthOutcome::Deny => {
                        pad_failure(started);
                        write_frame(
                            &mut conn,
                            &ReauthResponse::Deny {
                                reason: "Authentication failed".to_string(),
                            },
                        )?;
                    }
                    ReauthOutcome::Cancelled => write_frame(
                        &mut conn,
                        &ReauthResponse::Deny {
                            reason: "Authentication cancelled".to_string(),
                        },
                    )?,
                    // The client vanished mid-conversation; nothing left to reply to.
                    ReauthOutcome::Transport => return Ok(()),
                }
            }
            // A second Hello is harmless; re-acknowledge.
            ReauthRequest::Hello { .. } => write_frame(
                &mut conn,
                &ReauthResponse::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                },
            )?,
            // Nothing is in progress between attempts; a cancel is a no-op.
            ReauthRequest::Cancel => {}
            // A reply with no conversation in progress is a stray message.
            ReauthRequest::Reply { .. } => write_frame(
                &mut conn,
                &ReauthResponse::Error {
                    message: "no reauthentication in progress".to_string(),
                },
            )?,
        }
    }
}

/// The mandatory first exchange on the reauth seam: the client must send
/// [`ReauthRequest::Hello`] with a version this daemon speaks. Anything else, or an
/// incompatible version, ends the connection. Returns `true` on success.
fn reauth_handshake(conn: &mut UnixStream) -> Result<bool, FrameError> {
    let first: ReauthRequest = read_frame(conn)?;
    match first {
        ReauthRequest::Hello { protocol_version } if protocol_version == PROTOCOL_VERSION => {
            write_frame(
                conn,
                &ReauthResponse::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                },
            )?;
            Ok(true)
        }
        ReauthRequest::Hello { protocol_version } => {
            eprintln!(
                "doord: reauth: rejecting client speaking protocol v{protocol_version}; this daemon speaks v{PROTOCOL_VERSION}"
            );
            write_frame(
                conn,
                &ReauthResponse::Incompatible {
                    daemon_protocol_version: PROTOCOL_VERSION,
                },
            )?;
            Ok(false)
        }
        _ => {
            eprintln!("doord: reauth: client sent a request before the handshake; closing");
            write_frame(
                conn,
                &ReauthResponse::Error {
                    message: "handshake required before any other request".to_string(),
                },
            )?;
            Ok(false)
        }
    }
}

/// Whether a frame error means the client is gone or stalled (a clean disconnect, or a
/// read that hit its timeout) rather than a malformed frame — either way the connection
/// is dropped, but only a genuine frame error is worth an error reply.
fn is_client_gone(e: &FrameError) -> bool {
    matches!(
        e,
        FrameError::Io(io_err) if matches!(
            io_err.kind(),
            io::ErrorKind::UnexpectedEof
                | io::ErrorKind::WouldBlock
                | io::ErrorKind::TimedOut
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionReset
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::Secret;
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// A scripted reauthenticator: asks once for a secret over the *real* reauth wire
    /// types (exercising the framing) and allows only if it matches `password`, while
    /// recording the username it was asked to verify. It has no ability to spawn or open
    /// a session — the trait surface itself is verify-only — so a passing test proves the
    /// "no session, ever" invariant structurally, not by inspecting a launch log.
    struct ScriptedReauthFactory {
        password: String,
        /// Every username a begun attempt was asked to reauthenticate — the test reads
        /// this to assert the identity was the peer-cred one, never client input.
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl ReauthFactory for ScriptedReauthFactory {
        fn begin(&self) -> Box<dyn Reauthenticator> {
            Box::new(ScriptedReauth {
                password: self.password.clone(),
                calls: self.calls.clone(),
            })
        }
    }

    struct ScriptedReauth {
        password: String,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl Reauthenticator for ScriptedReauth {
        fn reauthenticate(&mut self, username: &str, client: &mut UnixStream) -> ReauthOutcome {
            self.calls.lock().unwrap().push(username.to_string());
            let question = ReauthResponse::Prompt(AuthPrompt::Question {
                text: "Password:".to_string(),
                secret: true,
            });
            if write_frame(client, &question).is_err() {
                return ReauthOutcome::Transport;
            }
            match read_frame::<_, ReauthRequest>(client) {
                Ok(ReauthRequest::Reply { response }) if response.expose() == self.password => {
                    ReauthOutcome::Allow
                }
                Ok(ReauthRequest::Reply { .. }) => ReauthOutcome::Deny,
                Ok(ReauthRequest::Cancel) => ReauthOutcome::Cancelled,
                Ok(_) => ReauthOutcome::Deny,
                Err(_) => ReauthOutcome::Transport,
            }
        }
    }

    /// The username the running test process's own uid resolves to — the identity the
    /// handler must bind every attempt to (a socketpair reports the creating process's
    /// uid on `SO_PEERCRED`).
    fn own_username() -> String {
        let uid = unsafe { libc::getuid() };
        user::resolve_uid(uid).expect("own uid must resolve").name
    }

    fn scripted(password: &str, calls: Arc<Mutex<Vec<String>>>) -> ScriptedReauthFactory {
        ScriptedReauthFactory {
            password: password.to_string(),
            calls,
        }
    }

    /// Drive a full handshake and one reauth attempt with the given typed password over
    /// an in-process socketpair, returning the terminal response and the recorded
    /// usernames the handler asked to verify.
    fn run_attempt(password_typed: &str) -> (ReauthResponse, Vec<String>) {
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let factory = scripted("hunter2", calls_in);
            let _ = handle_reauth_connection(server, &factory);
        });

        handshake(&mut client);

        write_frame(&mut client, &ReauthRequest::Begin).unwrap();
        assert_eq!(
            read_frame::<_, ReauthResponse>(&mut client).unwrap(),
            ReauthResponse::Prompt(AuthPrompt::Question {
                text: "Password:".to_string(),
                secret: true,
            })
        );
        write_frame(
            &mut client,
            &ReauthRequest::Reply {
                response: Secret::new(password_typed.to_string()),
            },
        )
        .unwrap();
        let terminal: ReauthResponse = read_frame(&mut client).unwrap();

        drop(client);
        handle.join().unwrap();
        let recorded = calls.lock().unwrap().clone();
        (terminal, recorded)
    }

    fn handshake(client: &mut UnixStream) {
        write_frame(
            client,
            &ReauthRequest::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .unwrap();
        assert_eq!(
            read_frame::<_, ReauthResponse>(client).unwrap(),
            ReauthResponse::Welcome {
                protocol_version: PROTOCOL_VERSION
            }
        );
    }

    #[test]
    fn correct_credential_allows_and_binds_to_the_peercred_uid() {
        let (terminal, calls) = run_attempt("hunter2");
        assert_eq!(terminal, ReauthResponse::Allow);
        // The identity verified was the peer-cred-derived username — the only thing the
        // handler can bind to, since the wire has no username field.
        assert_eq!(calls, vec![own_username()]);
    }

    #[test]
    fn wrong_credential_denies_with_a_coarse_reason() {
        let (terminal, calls) = run_attempt("wrong");
        assert!(
            matches!(terminal, ReauthResponse::Deny { .. }),
            "a wrong credential must deny, got {terminal:?}"
        );
        // Still bound to the peer-cred identity, never anything else.
        assert_eq!(calls, vec![own_username()]);
    }

    #[test]
    fn a_denied_attempt_is_padded_against_timing() {
        let start = Instant::now();
        let (terminal, _) = run_attempt("wrong");
        assert!(matches!(terminal, ReauthResponse::Deny { .. }));
        assert!(
            start.elapsed() >= crate::ipc::MIN_AUTH_FAILURE,
            "a denied reauth must be padded to at least the min-failure floor"
        );
    }

    #[test]
    fn cancel_during_the_conversation_denies() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let factory = scripted("hunter2", calls_in);
            let _ = handle_reauth_connection(server, &factory);
        });

        handshake(&mut client);
        write_frame(&mut client, &ReauthRequest::Begin).unwrap();
        let _prompt: ReauthResponse = read_frame(&mut client).unwrap();
        write_frame(&mut client, &ReauthRequest::Cancel).unwrap();
        let terminal: ReauthResponse = read_frame(&mut client).unwrap();
        assert!(
            matches!(terminal, ReauthResponse::Deny { .. }),
            "a cancel must resolve to a deny, got {terminal:?}"
        );
        drop(client);
        handle.join().unwrap();
    }

    #[test]
    fn no_session_is_ever_opened_on_a_successful_reauth() {
        // The reauth seam's only outcome is a verdict — there is no spawn/session-open
        // capability on the `Reauthenticator` trait at all. A successful attempt records
        // exactly one auth (a verify), and the response is `Allow`, which carries no
        // seat, pid, or session. This is the structural form of "no session, ever".
        let (terminal, calls) = run_attempt("hunter2");
        assert_eq!(terminal, ReauthResponse::Allow);
        assert_eq!(calls.len(), 1, "exactly one verify, and nothing spawned");
    }

    #[test]
    fn a_smuggled_target_username_is_rejected() {
        // The wire `Begin` is a unit variant (no fields). A client that tries to carry a
        // target identity — `{"Begin":{"username":"root"}}` — is rejected by
        // `deny_unknown_fields`/type-mismatch as a malformed frame, so no attempt runs
        // for anyone but the peer-cred uid.
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let factory = scripted("hunter2", calls_in);
            let _ = handle_reauth_connection(server, &factory);
        });

        handshake(&mut client);
        // Raw hand-rolled frame: length-prefixed JSON that smuggles a username.
        let body = br#"{"Begin":{"username":"root"}}"#;
        let mut raw = (body.len() as u32).to_be_bytes().to_vec();
        raw.extend_from_slice(body);
        use std::io::Write;
        client.write_all(&raw).unwrap();
        client.flush().unwrap();

        // The daemon rejects it as malformed and closes; no reauth ran for "root".
        let resp: Result<ReauthResponse, _> = read_frame(&mut client);
        assert!(
            matches!(resp, Ok(ReauthResponse::Error { .. })) || resp.is_err(),
            "a smuggled username must be refused, got {resp:?}"
        );
        drop(client);
        handle.join().unwrap();
        assert!(
            calls.lock().unwrap().is_empty(),
            "no attempt may run for a client-supplied identity"
        );
    }

    #[test]
    fn a_request_before_the_handshake_is_refused() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let factory = scripted("hunter2", calls_in);
            let _ = handle_reauth_connection(server, &factory);
        });

        // Skip Hello, go straight to Begin.
        write_frame(&mut client, &ReauthRequest::Begin).unwrap();
        let resp: ReauthResponse = read_frame(&mut client).unwrap();
        assert!(
            matches!(resp, ReauthResponse::Error { .. }),
            "a pre-handshake request must be refused, got {resp:?}"
        );
        drop(client);
        handle.join().unwrap();
        assert!(
            calls.lock().unwrap().is_empty(),
            "no attempt may run before the handshake"
        );
    }

    #[test]
    fn an_incompatible_version_is_rejected() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let factory = scripted("hunter2", calls_in);
            let _ = handle_reauth_connection(server, &factory);
        });

        write_frame(
            &mut client,
            &ReauthRequest::Hello {
                protocol_version: PROTOCOL_VERSION + 99,
            },
        )
        .unwrap();
        let resp: ReauthResponse = read_frame(&mut client).unwrap();
        assert_eq!(
            resp,
            ReauthResponse::Incompatible {
                daemon_protocol_version: PROTOCOL_VERSION
            }
        );
        drop(client);
        handle.join().unwrap();
    }

    #[test]
    fn a_malformed_frame_is_refused_without_echo() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in = calls.clone();
        let handle = thread::spawn(move || {
            let factory = scripted("hunter2", calls_in);
            let _ = handle_reauth_connection(server, &factory);
        });

        handshake(&mut client);
        // Well-framed non-JSON garbage after the handshake.
        let body = b"not json at all";
        let mut raw = (body.len() as u32).to_be_bytes().to_vec();
        raw.extend_from_slice(body);
        use std::io::Write;
        client.write_all(&raw).unwrap();
        client.flush().unwrap();

        let resp: Result<ReauthResponse, _> = read_frame(&mut client);
        assert!(
            matches!(resp, Ok(ReauthResponse::Error { .. })) || resp.is_err(),
            "a malformed frame must be refused, got {resp:?}"
        );
        drop(client);
        handle.join().unwrap();
    }
}
