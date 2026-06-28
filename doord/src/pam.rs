//! The daemon side of a login: spawning the per-login session worker and
//! proxying its PAM conversation to the greeter.
//!
//! Per D-0005 the daemon never runs PAM itself — it re-execs itself as a
//! short-lived [worker](crate::worker) that owns the PAM transaction and is the
//! logind session leader. This module is the daemon's handle to that worker:
//! [`WorkerLogin`] forks the worker, relays the worker's
//! [`WorkerEvent::Prompt`](crate::worker::WorkerEvent) prompts to the greeter as
//! [`AuthPrompt`] frames and the greeter's reply back as a
//! [`WorkerCommand::Reply`](crate::worker::WorkerCommand), and on `Start` tells
//! the worker to launch — then hands back a [`SessionChild`] that waits on the
//! worker (which lives exactly as long as the session it leads).
//!
//! All greeter wire-protocol framing stays here in the daemon (the trust
//! boundary, D-0003); the worker speaks only the private daemon↔worker codec and
//! cannot reach a greeter byte (the greeter socket is `O_CLOEXEC` and is closed
//! by the worker's `exec`).
//!
//! The [`Login`]/[`LoginFactory`] seam is abstracted so the IPC flow can be tested
//! in-process without root, a live PAM stack, or a real worker.

use std::io;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

use protocol::{read_frame, write_frame, AuthPrompt, Request, Response};

use crate::config::SeatTarget;
use crate::sessions::{DiscoveredSession, SessionKind};
use crate::spawn::{LaunchError, SessionChild};
use crate::worker::{
    self, Verdict, WorkerCommand, WorkerEvent, WorkerSeat, WorkerSession, CONTROL_FD,
};

/// The result of an authentication attempt, as the daemon will report it.
#[derive(Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Credentials accepted and the account is permitted to log in.
    Success,
    /// Authentication or account checks rejected the attempt. The reason is
    /// deliberately coarse — the detailed PAM error is journaled by the worker,
    /// never sent to the login screen (it could reveal whether an account exists).
    Failure,
    /// The user asked to cancel the in-progress conversation.
    Cancelled,
    /// The worker or greeter connection broke mid-conversation; drop the login.
    Transport,
}

impl From<Verdict> for AuthOutcome {
    fn from(v: Verdict) -> Self {
        match v {
            Verdict::Success => AuthOutcome::Success,
            Verdict::Failure => AuthOutcome::Failure,
            Verdict::Cancelled => AuthOutcome::Cancelled,
            Verdict::Transport => AuthOutcome::Transport,
        }
    }
}

/// One greeter's login. Created per connection; drives auth and then the session.
pub trait Login {
    /// Run one authentication conversation to a terminal outcome. On
    /// [`AuthOutcome::Success`] the login is bound to `username` and [`start`]
    /// may be called; the greeter may retry after any non-success outcome.
    ///
    /// [`start`]: Login::start
    fn authenticate(&mut self, username: &str) -> AuthOutcome;

    /// The user bound by a prior successful [`authenticate`], if any. The
    /// session-start gate reads this: no authenticated user, no launch.
    ///
    /// [`authenticate`]: Login::authenticate
    fn user(&self) -> Option<&str>;

    /// Launch `session` on `seat` as the authenticated user. Only valid after a
    /// successful [`authenticate`]. Returns a handle that waits on the running
    /// session (for the worker-backed login, the worker process — which exits when
    /// the session ends, after closing the logind session).
    ///
    /// [`authenticate`]: Login::authenticate
    fn start(
        &mut self,
        session: &DiscoveredSession,
        seat: &SeatTarget,
    ) -> Result<SessionChild, LaunchError>;
}

/// Builds a fresh [`Login`] per greeter connection, handing it that connection's
/// socket (a clone the login owns for relaying the PAM conversation).
pub trait LoginFactory {
    fn begin(&self, greeter: UnixStream) -> Box<dyn Login>;
}

/// Production [`LoginFactory`]: each login re-execs the daemon as a session
/// worker. The worker reads its configuration (PAM service, seat, …) from the
/// inherited environment, so the factory itself carries no state.
pub struct WorkerLoginFactory;

impl WorkerLoginFactory {
    pub fn new() -> Self {
        WorkerLoginFactory
    }
}

impl Default for WorkerLoginFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginFactory for WorkerLoginFactory {
    fn begin(&self, greeter: UnixStream) -> Box<dyn Login> {
        match spawn_worker() {
            Ok((child, control)) => Box::new(WorkerLogin {
                greeter,
                control,
                child: Some(child),
                username: None,
            }),
            Err(e) => {
                eprintln!("doord: could not start the session worker: {e}");
                Box::new(DeadLogin)
            }
        }
    }
}

/// A worker-backed login: the daemon's end of the control socket plus the worker
/// process and the greeter socket it relays the conversation over.
pub struct WorkerLogin {
    greeter: UnixStream,
    control: UnixStream,
    /// The worker process. Taken at [`start`](Login::start) and wrapped in the
    /// returned [`SessionChild`]; the daemon waits on it for the session lifetime.
    child: Option<Child>,
    username: Option<String>,
}

impl Login for WorkerLogin {
    fn authenticate(&mut self, username: &str) -> AuthOutcome {
        if write_frame(
            &mut self.control,
            &WorkerCommand::Auth {
                username: username.to_string(),
            },
        )
        .is_err()
        {
            return AuthOutcome::Transport;
        }

        loop {
            let event = match read_frame::<_, WorkerEvent>(&mut self.control) {
                Ok(event) => event,
                // The worker died; nothing more will come.
                Err(_) => return AuthOutcome::Transport,
            };

            match event {
                WorkerEvent::Prompt { text, secret } => {
                    let question = Response::Auth(AuthPrompt::Question { text, secret });
                    if write_frame(&mut self.greeter, &question).is_err() {
                        let _ = write_frame(&mut self.control, &WorkerCommand::Cancel);
                        return AuthOutcome::Transport;
                    }
                    match read_frame::<_, Request>(&mut self.greeter) {
                        Ok(Request::AuthReply { response }) => {
                            if write_frame(&mut self.control, &WorkerCommand::Reply { response })
                                .is_err()
                            {
                                return AuthOutcome::Transport;
                            }
                        }
                        // A cancel or any other frame ends the conversation; tell
                        // the worker to abort it.
                        Ok(_) => {
                            let _ = write_frame(&mut self.control, &WorkerCommand::Cancel);
                        }
                        Err(_) => {
                            let _ = write_frame(&mut self.control, &WorkerCommand::Cancel);
                            return AuthOutcome::Transport;
                        }
                    }
                }
                WorkerEvent::Info { text } => {
                    let _ = write_frame(
                        &mut self.greeter,
                        &Response::Auth(AuthPrompt::Info { text }),
                    );
                }
                WorkerEvent::Error { text } => {
                    let _ = write_frame(
                        &mut self.greeter,
                        &Response::Auth(AuthPrompt::Error { text }),
                    );
                }
                WorkerEvent::Auth(verdict) => {
                    let outcome = AuthOutcome::from(verdict);
                    if outcome == AuthOutcome::Success {
                        self.username = Some(username.to_string());
                    }
                    return outcome;
                }
                // Session events have no place during authentication.
                WorkerEvent::Started | WorkerEvent::StartFailed => return AuthOutcome::Failure,
            }
        }
    }

    fn user(&self) -> Option<&str> {
        self.username.as_deref()
    }

    fn start(
        &mut self,
        session: &DiscoveredSession,
        seat: &SeatTarget,
    ) -> Result<SessionChild, LaunchError> {
        let command = WorkerCommand::Start {
            session: WorkerSession {
                id: session.id.clone(),
                exec: session.exec.clone(),
                session_type: match session.kind {
                    SessionKind::Wayland => "wayland",
                    SessionKind::X11 => "x11",
                }
                .to_string(),
            },
            seat: WorkerSeat {
                seat: seat.seat.clone(),
                vtnr: seat.vtnr,
            },
        };
        if let Err(e) = write_frame(&mut self.control, &command) {
            return Err(LaunchError::Spawn(io::Error::other(format!(
                "telling the worker to start failed: {e}"
            ))));
        }

        loop {
            match read_frame::<_, WorkerEvent>(&mut self.control) {
                Ok(WorkerEvent::Started) => {
                    let child = self.child.take().ok_or_else(|| {
                        LaunchError::Spawn(io::Error::other("session worker already consumed"))
                    })?;
                    return Ok(SessionChild::new(child));
                }
                Ok(WorkerEvent::StartFailed) => {
                    return Err(LaunchError::Spawn(io::Error::other(
                        "the session worker refused the launch",
                    )));
                }
                // Stray events before the terminal one: ignore.
                Ok(_) => {}
                Err(e) => {
                    return Err(LaunchError::Spawn(io::Error::other(format!(
                        "the session worker vanished: {e}"
                    ))));
                }
            }
        }
    }
}

/// Re-exec the daemon as a session worker, returning the worker process and the
/// daemon's end of the control socket. The worker's end is dup'd onto
/// [`CONTROL_FD`] (kept open across `exec`); every other fd — listener, greeter
/// socket — is `O_CLOEXEC` and is closed by the `exec`, so the worker is reachable
/// only over the control socket and can never touch a greeter byte.
fn spawn_worker() -> io::Result<(Child, UnixStream)> {
    let (daemon_end, worker_end) = UnixStream::pair()?;
    let worker_fd = worker_end.as_raw_fd();

    let mut command = Command::new("/proc/self/exe");
    command.arg(worker::WORKER_ARG);
    // SAFETY: the closure runs in the forked child before `exec`. It only dup's an
    // inherited fd onto a fixed number and clears that fd's close-on-exec flag —
    // async-signal-safe syscalls touching no shared parent state.
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(worker_fd, CONTROL_FD) < 0 {
                return Err(io::Error::last_os_error());
            }
            // dup2 clears CLOEXEC on the new fd; set it explicitly too in case the
            // inherited fd already was CONTROL_FD (then dup2 is a no-op).
            if libc::fcntl(CONTROL_FD, libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = command.spawn()?;
    // The parent keeps only its end; the child's inherited copy is CLOEXEC and is
    // gone after exec — only the dup'd CONTROL_FD survives there.
    drop(worker_end);
    Ok((child, daemon_end))
}

/// A login whose worker could not be started: every operation fails closed, so a
/// worker-spawn failure degrades to "cannot authenticate / cannot start", never
/// to an unauthenticated launch.
struct DeadLogin;

impl Login for DeadLogin {
    fn authenticate(&mut self, _username: &str) -> AuthOutcome {
        AuthOutcome::Transport
    }
    fn user(&self) -> Option<&str> {
        None
    }
    fn start(
        &mut self,
        _session: &DiscoveredSession,
        _seat: &SeatTarget,
    ) -> Result<SessionChild, LaunchError> {
        Err(LaunchError::Spawn(io::Error::other(
            "the session worker is unavailable",
        )))
    }
}

/// Test login seam: a scripted [`LoginFactory`]/[`Login`] that asks once for a
/// secret over the greeter socket (exercising the real IPC framing) and accepts
/// only if it matches `password`, then records launches instead of forking a
/// worker. Lets the connection's auth-gating and identity-binding be exercised end
/// to end without root, a live PAM stack, or a real session spawn.
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub struct ScriptedLoginFactory {
        pub password: String,
        /// Shared log of every (session id, username) a begun login was asked to
        /// start — the test thread reads this to assert what was launched.
        pub calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl LoginFactory for ScriptedLoginFactory {
        fn begin(&self, greeter: UnixStream) -> Box<dyn Login> {
            Box::new(ScriptedLogin {
                password: self.password.clone(),
                greeter,
                user: None,
                calls: self.calls.clone(),
            })
        }
    }

    struct ScriptedLogin {
        password: String,
        greeter: UnixStream,
        user: Option<String>,
        calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl Login for ScriptedLogin {
        fn authenticate(&mut self, username: &str) -> AuthOutcome {
            let question = Response::Auth(AuthPrompt::Question {
                text: "Password:".to_string(),
                secret: true,
            });
            if write_frame(&mut self.greeter, &question).is_err() {
                return AuthOutcome::Transport;
            }
            match read_frame::<_, Request>(&mut self.greeter) {
                Ok(Request::AuthReply { response }) if response.expose() == self.password => {
                    self.user = Some(username.to_string());
                    AuthOutcome::Success
                }
                Ok(Request::AuthReply { .. }) => AuthOutcome::Failure,
                Ok(Request::CancelAuth) => AuthOutcome::Cancelled,
                Ok(_) => AuthOutcome::Failure,
                Err(_) => AuthOutcome::Transport,
            }
        }

        fn user(&self) -> Option<&str> {
            self.user.as_deref()
        }

        fn start(
            &mut self,
            session: &DiscoveredSession,
            _seat: &SeatTarget,
        ) -> Result<SessionChild, LaunchError> {
            let user = self
                .user
                .clone()
                .expect("start is only reached after a successful authenticate");
            self.calls.lock().unwrap().push((session.id.clone(), user));
            Ok(SessionChild::detached())
        }
    }
}
