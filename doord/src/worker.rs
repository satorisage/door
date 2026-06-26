//! The per-login session worker — the process that actually holds a PAM
//! transaction and *is* the logind session leader.
//!
//! Why a separate process (D-0005): `pam_systemd` registers a logind session by
//! migrating the **caller** into the session's cgroup scope. If the caller were
//! the long-lived daemon, the daemon would be trapped in the first session's
//! scope, the session would never close, and no second session could ever be
//! registered. So for each login the daemon re-execs itself as this short-lived
//! worker; the worker opens the session (and is therefore the leader), spawns the
//! session as its child, waits for it, closes the PAM session, and exits — at
//! which point the scope empties and logind reaps it. The daemon never enters any
//! session scope and can serve login after login.
//!
//! The worker never touches a greeter byte. The daemon owns all wire-protocol
//! framing (the trust boundary, D-0003); it re-execs the worker with the greeter
//! socket closed (it is `O_CLOEXEC`), passing only a control socket. The PAM
//! conversation is proxied: the worker emits [`WorkerEvent::Prompt`] over the
//! control socket and the daemon relays the typed reply back as
//! [`WorkerCommand::Reply`]. The worker speaks only this small private codec.

use std::ffi::{CStr, CString, OsString};
use std::io;
use std::os::unix::io::FromRawFd;
use std::os::unix::net::UnixStream;
use std::process::ExitCode;

use pam_client::{Context, ConversationHandler, ErrorCode, Flag, SessionToken};
use protocol::{read_frame, write_frame, Secret};
use serde::{Deserialize, Serialize};

use crate::config::{Config, SeatTarget};
use crate::spawn::{self, SessionChild};
use crate::user;

/// argv[1] that selects worker mode when the daemon re-execs itself.
pub const WORKER_ARG: &str = "session-worker";

/// The fd the daemon dup's the control socket onto before `exec`, where the
/// worker picks it up. Not `O_CLOEXEC`, unlike every other fd, so it survives the
/// re-exec; the greeter socket and listener do not.
pub const CONTROL_FD: i32 = 3;

/// Daemon → worker. The only inputs the worker accepts; it never reads the
/// greeter directly.
#[derive(Serialize, Deserialize)]
pub enum WorkerCommand {
    /// Run a PAM authentication for `username` (conversation proxied back).
    Auth { username: String },
    /// The greeter's typed reply to the in-flight [`WorkerEvent::Prompt`].
    Reply { response: Secret },
    /// The greeter cancelled the in-flight conversation.
    Cancel,
    /// Open the logind session and launch `session` on `seat`, as the user the
    /// worker authenticated. Carries only a session id and its `Exec` (resolved
    /// daemon-side) — never an identity.
    Start {
        session: WorkerSession,
        seat: WorkerSeat,
    },
}

/// The startable session, projected for the worker (the daemon resolved it from
/// discovery; the `Exec` argv crosses here, the greeter never sees it).
#[derive(Serialize, Deserialize)]
pub struct WorkerSession {
    pub id: String,
    pub exec: Vec<String>,
    /// `"wayland"` or `"x11"` — becomes `XDG_SESSION_TYPE`.
    pub session_type: String,
}

/// The seat/VT the session registers on (door's own, never the greeter's).
#[derive(Serialize, Deserialize)]
pub struct WorkerSeat {
    pub seat: String,
    pub vtnr: Option<u32>,
}

/// Worker → daemon.
#[derive(Serialize, Deserialize)]
pub enum WorkerEvent {
    /// PAM wants input; the daemon turns this into an `AuthPrompt` to the greeter.
    Prompt { text: String, secret: bool },
    /// A one-way PAM message for the greeter.
    Info { text: String },
    /// A one-way PAM error message for the greeter.
    Error { text: String },
    /// Terminal authentication verdict.
    Auth(Verdict),
    /// The session was launched (the daemon may tell the greeter, then wait on
    /// the worker for the session's lifetime).
    Started,
    /// The launch failed (unknown reason to the greeter; detail journaled here).
    StartFailed,
}

/// The four authentication outcomes, on the wire between worker and daemon.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum Verdict {
    Success,
    Failure,
    Cancelled,
    Transport,
}

/// Worker entry point (when the binary is run with the [`WORKER_ARG`] argument).
/// Re-applies the process hardening baseline — `exec` reset the dumpable flag,
/// and this process is about to handle a password — then serves the daemon over
/// the control socket until the login is done.
pub fn main() -> ExitCode {
    crate::hardening::apply_baseline();
    let config = Config::from_env();

    // SAFETY: the daemon dup'd the control socket onto CONTROL_FD before exec and
    // we are the sole owner of it here; no other code touches this fd.
    let control = unsafe { UnixStream::from_raw_fd(CONTROL_FD) };

    match serve(control, &config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("doord-worker: fatal: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The session that has authenticated: the live PAM context, the user it is bound
/// to, and — once a session is opened — the token that closes it on drop.
struct AuthedSession {
    context: Context<ControlConversation>,
    username: String,
    /// `Some` after [`open_session`](Context::open_session); closed when this
    /// worker exits (i.e. when the session it leads has ended).
    session: Option<SessionToken>,
}

impl Drop for AuthedSession {
    fn drop(&mut self) {
        if let Some(token) = self.session.take() {
            let session = self.context.unleak_session(token);
            if let Err(e) = session.close(Flag::NONE) {
                eprintln!(
                    "doord-worker: closing PAM session for '{}' failed: {}",
                    self.username, e
                );
            }
        }
    }
}

/// Read commands from the daemon and act on them until the login completes (a
/// session started and ended) or the daemon closes the control socket.
fn serve(mut control: UnixStream, config: &Config) -> io::Result<()> {
    let mut authed: Option<AuthedSession> = None;

    loop {
        let command = match read_frame::<_, WorkerCommand>(&mut control) {
            Ok(cmd) => cmd,
            // The daemon closed the control socket: the connection ended without
            // a session (e.g. the greeter disconnected). Nothing more to do.
            Err(_) => return Ok(()),
        };

        match command {
            WorkerCommand::Auth { username } => {
                let verdict = authenticate(config, &username, &control, &mut authed);
                // A failed write means the daemon is gone; nothing left to serve.
                if write_frame(&mut control, &WorkerEvent::Auth(verdict)).is_err() {
                    return Ok(());
                }
            }
            WorkerCommand::Start { session, seat } => {
                let txn = match authed.as_mut() {
                    Some(txn) => txn,
                    None => {
                        // The daemon gates Start on auth; reaching here unauthed is
                        // an internal error, reported as a generic failure.
                        let _ = write_frame(&mut control, &WorkerEvent::StartFailed);
                        continue;
                    }
                };
                match start_session(txn, &session, &seat) {
                    Ok(child) => {
                        if write_frame(&mut control, &WorkerEvent::Started).is_err() {
                            return Ok(());
                        }
                        // We are the session leader; own it until it exits, then
                        // fall out of the loop so `authed` drops and closes the
                        // PAM session — emptying the scope so logind reaps it.
                        if let Err(e) = child.wait() {
                            eprintln!("doord-worker: waiting on the session failed: {e}");
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("doord-worker: could not start session '{}': {e}", session.id);
                        let _ = write_frame(&mut control, &WorkerEvent::StartFailed);
                    }
                }
            }
            // A reply or cancel with no conversation in progress is stray.
            WorkerCommand::Reply { .. } | WorkerCommand::Cancel => {}
        }
    }
}

/// Run one PAM authentication for `username`, proxying the conversation to the
/// daemon over a clone of the control socket. On success the worker keeps the
/// live context (in `authed`) for the later session open.
fn authenticate(
    config: &Config,
    username: &str,
    control: &UnixStream,
    authed: &mut Option<AuthedSession>,
) -> Verdict {
    let proxy = match control.try_clone() {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("doord-worker: could not clone control socket: {e}");
            return Verdict::Transport;
        }
    };
    let conv = ControlConversation {
        control: proxy,
        fault: None,
        phase_session: false,
    };

    let mut context = match Context::new(&config.pam_service, Some(username), conv) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("doord-worker: pam_start failed for '{}': {e}", config.pam_service);
            return Verdict::Failure;
        }
    };

    let auth_result = context.authenticate(Flag::NONE);

    // A transport break or cancel during the conversation takes precedence over
    // whatever PAM returned for the aborted attempt.
    if let Some(fault) = context.conversation_mut().fault.take() {
        return fault;
    }
    if let Err(e) = auth_result {
        eprintln!("doord-worker: authentication failed for '{username}': {e}");
        return Verdict::Failure;
    }
    if let Err(e) = context.acct_mgmt(Flag::NONE) {
        eprintln!("doord-worker: account check denied '{username}': {e}");
        return Verdict::Failure;
    }

    // Past auth: any further PAM prompt belongs to the session phase, where the
    // greeter is gone — the conversation refuses prompts from here on.
    context.conversation_mut().phase_session = true;
    *authed = Some(AuthedSession {
        context,
        username: username.to_string(),
        session: None,
    });
    Verdict::Success
}

/// Open the logind session for the authenticated user and launch the chosen
/// session as this worker's child. Because *this* process calls
/// `pam_open_session`, this process is the logind session leader.
fn start_session(
    txn: &mut AuthedSession,
    session: &WorkerSession,
    seat: &WorkerSeat,
) -> io::Result<SessionChild> {
    let target = user::resolve(&txn.username)?;
    let seat_target = SeatTarget {
        seat: seat.seat.clone(),
        vtnr: seat.vtnr,
    };

    // Tell logind (via pam_systemd, in the session phase) which seat/VT/kind this
    // login is. door's own seat/VT, never the greeter's.
    putenv(txn, &format!("XDG_SEAT={}", seat.seat))?;
    if let Some(vtnr) = seat.vtnr {
        putenv(txn, &format!("XDG_VTNR={vtnr}"))?;
        // PAM_TTY feeds the audit trail and gives pam_systemd a second VT source;
        // best-effort since the seat assignment rides on XDG_VTNR.
        if let Err(e) = txn.context.set_tty(Some(&format!("/dev/tty{vtnr}"))) {
            eprintln!("doord-worker: could not set PAM_TTY to /dev/tty{vtnr}: {e}");
        }
    }
    putenv(txn, &format!("XDG_SESSION_TYPE={}", session.session_type))?;
    putenv(txn, "XDG_SESSION_CLASS=user")?;
    putenv(txn, &format!("XDG_SESSION_DESKTOP={}", session.id))?;

    let pam_session = txn
        .context
        .open_session(Flag::NONE)
        .map_err(|e| io::Error::other(format!("pam_open_session: {e}")))?;

    let pam_env: Vec<(OsString, OsString)> = pam_session.envlist().into();
    txn.session = Some(pam_session.leak());

    spawn::launch(&session.exec, &target, &pam_env, &seat_target)
        .map_err(|e| io::Error::other(format!("{e}")))
}

/// Set a PAM environment variable, mapping a failure to an io error.
fn putenv(txn: &mut AuthedSession, name_value: &str) -> io::Result<()> {
    txn.context
        .putenv(name_value)
        .map_err(|e| io::Error::other(format!("pam_putenv: {e}")))
}

/// PAM conversation that relays to the daemon over the control socket (which then
/// relays to the greeter). It owns its own clone of the control socket so the
/// context can outlive any single command without holding the worker's main
/// command-read loop — the two never read concurrently (the loop is blocked
/// inside `authenticate` while the conversation runs).
struct ControlConversation {
    control: UnixStream,
    /// Latched on the first failed round-trip so the outer `authenticate` can
    /// distinguish a cancel/transport break from a real auth failure.
    fault: Option<Verdict>,
    /// Once true (post-auth), prompts are refused rather than sent to a greeter
    /// that has moved on; messages are journaled.
    phase_session: bool,
}

impl ControlConversation {
    fn ask(&mut self, prompt: &CStr, secret: bool) -> Result<CString, ErrorCode> {
        if self.fault.is_some() {
            return Err(ErrorCode::CONV_ERR);
        }
        if self.phase_session {
            eprintln!("doord-worker: suppressed a PAM session-phase prompt");
            return Err(ErrorCode::CONV_ERR);
        }
        let text = prompt.to_string_lossy().into_owned();
        if write_frame(&mut self.control, &WorkerEvent::Prompt { text, secret }).is_err() {
            self.fault = Some(Verdict::Transport);
            return Err(ErrorCode::CONV_ERR);
        }
        match read_frame::<_, WorkerCommand>(&mut self.control) {
            Ok(WorkerCommand::Reply { response }) => {
                // Widen to the CString PAM needs only here, at the last moment.
                // A NUL in the secret can't be a valid password; reject it.
                let result = CString::new(response.expose()).map_err(|_| ErrorCode::CONV_ERR);
                // `response` (a Secret) zeroizes as it drops at end of scope.
                result
            }
            Ok(WorkerCommand::Cancel) => {
                self.fault = Some(Verdict::Cancelled);
                Err(ErrorCode::CONV_ERR)
            }
            Ok(_) | Err(_) => {
                self.fault = Some(Verdict::Transport);
                Err(ErrorCode::CONV_ERR)
            }
        }
    }

    fn notify(&mut self, event: WorkerEvent) {
        if self.fault.is_some() || self.phase_session {
            if self.phase_session {
                eprintln!("doord-worker: PAM session message (journaled): {event:?}");
            }
            return;
        }
        let _ = write_frame(&mut self.control, &event);
    }
}

impl ConversationHandler for ControlConversation {
    fn prompt_echo_on(&mut self, prompt: &CStr) -> Result<CString, ErrorCode> {
        self.ask(prompt, false)
    }

    fn prompt_echo_off(&mut self, prompt: &CStr) -> Result<CString, ErrorCode> {
        self.ask(prompt, true)
    }

    fn text_info(&mut self, msg: &CStr) {
        self.notify(WorkerEvent::Info {
            text: msg.to_string_lossy().into_owned(),
        });
    }

    fn error_msg(&mut self, msg: &CStr) {
        self.notify(WorkerEvent::Error {
            text: msg.to_string_lossy().into_owned(),
        });
    }
}

// `WorkerEvent` is logged via Debug in the session-phase journal path above.
impl std::fmt::Debug for WorkerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerEvent::Prompt { secret, .. } => write!(f, "Prompt {{ secret: {secret} }}"),
            WorkerEvent::Info { text } => write!(f, "Info {{ {text:?} }}"),
            WorkerEvent::Error { text } => write!(f, "Error {{ {text:?} }}"),
            WorkerEvent::Auth(_) => write!(f, "Auth"),
            WorkerEvent::Started => write!(f, "Started"),
            WorkerEvent::StartFailed => write!(f, "StartFailed"),
        }
    }
}
