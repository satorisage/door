//! The greeter's client end of the door wire protocol.
//!
//! A thin, synchronous wrapper over the daemon socket: it dials `DOORD_SOCKET`,
//! runs the mandatory version handshake, and exposes the conversation as typed
//! steps — list the sessions, drive the PAM dialogue prompt by prompt, start the
//! chosen session, ask for power actions. It holds **no** authority: every method
//! is a request the daemon is free to refuse, and the only secret it touches is
//! the user's reply, which it forwards as a [`Secret`] and never stores.
//!
//! Blocking I/O lives here on purpose. The UI runs this client on a background
//! reader so the socket's request→response rhythm never blocks the render loop;
//! keeping the protocol logic synchronous makes it straightforward to unit-test
//! against a scripted daemon over a socket pair.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;

use protocol::{
    read_frame, write_frame, AuthPrompt, FrameError, PowerAction, Request, Response, Secret,
    Session, PROTOCOL_VERSION,
};

/// Default production socket — matches the daemon's `DEFAULT_SOCKET_PATH`.
pub const DEFAULT_SOCKET: &str = "/run/doord/door.sock";

/// Why a client operation could not complete.
#[derive(Debug)]
pub enum ClientError {
    /// The socket failed (not present, closed, timed out, permission).
    Io(io::Error),
    /// A frame could not be written or parsed.
    Frame(FrameError),
    /// The daemon cannot speak our protocol version; it sent `Incompatible`.
    Incompatible { daemon_version: u32 },
    /// The daemon sent a response that does not belong at this point in the
    /// conversation (a protocol violation on the daemon side, or our own bug).
    Unexpected(Response),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Io(e) => write!(f, "socket error: {e}"),
            ClientError::Frame(e) => write!(f, "protocol framing error: {e}"),
            ClientError::Incompatible { daemon_version } => write!(
                f,
                "the daemon speaks protocol v{daemon_version}; this greeter speaks v{PROTOCOL_VERSION}"
            ),
            ClientError::Unexpected(response) => {
                write!(f, "the daemon sent an unexpected response: {response:?}")
            }
        }
    }
}

impl std::error::Error for ClientError {}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> Self {
        ClientError::Io(e)
    }
}

impl From<FrameError> for ClientError {
    fn from(e: FrameError) -> Self {
        ClientError::Frame(e)
    }
}

type Result<T> = std::result::Result<T, ClientError>;

/// One step the daemon surfaces during an authentication conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStep {
    /// PAM wants input — render `text`; mask it if `secret`. Answer with
    /// [`Client::reply`].
    Question { text: String, secret: bool },
    /// A one-way informational message to show the user.
    Info(String),
    /// A one-way error message to show the user.
    Error(String),
    /// Authentication succeeded; the chosen session may now be started.
    Success,
    /// Authentication failed (the reason is safe to show); the user may retry.
    Failure(String),
}

/// The outcome of asking the daemon to start a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartOutcome {
    /// The session launched; the greeter should step aside.
    Started,
    /// The daemon refused (the message is non-leaky, safe to show).
    Refused(String),
}

/// A connected, handshaken client.
#[derive(Debug)]
pub struct Client {
    stream: UnixStream,
}

impl Client {
    /// Dial `socket_path` and complete the version handshake.
    pub fn connect(socket_path: impl AsRef<Path>) -> Result<Client> {
        let stream = UnixStream::connect(socket_path.as_ref())?;
        Client::over(stream)
    }

    /// Build a client over an already-connected stream and handshake. Used by the
    /// production [`connect`](Client::connect) and by tests over a socket pair.
    fn over(stream: UnixStream) -> Result<Client> {
        let mut client = Client { stream };
        client.handshake()?;
        Ok(client)
    }

    /// Send `Hello` and require a matching `Welcome` before anything else.
    fn handshake(&mut self) -> Result<()> {
        self.send(&Request::Hello {
            protocol_version: PROTOCOL_VERSION,
        })?;
        match self.recv()? {
            Response::Welcome { .. } => Ok(()),
            Response::Incompatible {
                daemon_protocol_version,
            } => Err(ClientError::Incompatible {
                daemon_version: daemon_protocol_version,
            }),
            other => Err(ClientError::Unexpected(other)),
        }
    }

    /// Ask for the startable sessions.
    pub fn list_sessions(&mut self) -> Result<Vec<Session>> {
        self.send(&Request::ListSessions)?;
        match self.recv()? {
            Response::Sessions(sessions) => Ok(sessions),
            other => Err(ClientError::Unexpected(other)),
        }
    }

    /// Begin an authentication conversation for `username`. Follow with
    /// [`recv_auth`](Client::recv_auth) to read the daemon's prompts.
    pub fn begin_auth(&mut self, username: &str) -> Result<()> {
        self.send(&Request::BeginAuth {
            username: username.to_string(),
        })
    }

    /// Read the next conversation step from the daemon.
    pub fn recv_auth(&mut self) -> Result<AuthStep> {
        match self.recv()? {
            Response::Auth(AuthPrompt::Question { text, secret }) => {
                Ok(AuthStep::Question { text, secret })
            }
            Response::Auth(AuthPrompt::Info { text }) => Ok(AuthStep::Info(text)),
            Response::Auth(AuthPrompt::Error { text }) => Ok(AuthStep::Error(text)),
            Response::AuthSuccess => Ok(AuthStep::Success),
            Response::AuthFailure { reason } => Ok(AuthStep::Failure(reason)),
            other => Err(ClientError::Unexpected(other)),
        }
    }

    /// Answer the most recent [`AuthStep::Question`]. The secret is moved in and
    /// dropped (zeroized) once written.
    pub fn reply(&mut self, response: Secret) -> Result<()> {
        self.send(&Request::AuthReply { response })
    }

    /// Abandon the in-progress conversation. Part of the complete protocol
    /// surface; wired to a UI affordance when the greeter grows a cancel button.
    #[allow(dead_code)]
    pub fn cancel_auth(&mut self) -> Result<()> {
        self.send(&Request::CancelAuth)
    }

    /// After a success, start `session_id` for the authenticated user.
    pub fn start(&mut self, session_id: &str) -> Result<StartOutcome> {
        self.send(&Request::Start {
            session_id: session_id.to_string(),
        })?;
        match self.recv()? {
            Response::Started => Ok(StartOutcome::Started),
            Response::Error { message } => Ok(StartOutcome::Refused(message)),
            other => Err(ClientError::Unexpected(other)),
        }
    }

    /// Ask the daemon to perform a power action. Returns the daemon's message if
    /// it refused (e.g. not yet available); a successful poweroff/reboot ends the
    /// connection rather than replying.
    pub fn power(&mut self, action: PowerAction) -> Result<Option<String>> {
        self.send(&Request::Power(action))?;
        match self.recv()? {
            Response::Error { message } => Ok(Some(message)),
            other => Err(ClientError::Unexpected(other)),
        }
    }

    fn send(&mut self, request: &Request) -> Result<()> {
        write_frame(&mut self.stream, request)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Response> {
        Ok(read_frame::<_, Response>(&mut self.stream)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    /// Spawn a scripted "daemon" on one end of a socket pair: it runs the
    /// handshake, then hands the connected stream to `script` to drive responses.
    /// Returns a handshaken [`Client`] on the other end.
    fn with_daemon<F>(script: F) -> Client
    where
        F: FnOnce(&mut UnixStream) + Send + 'static,
    {
        let (client_end, mut server_end) = UnixStream::pair().unwrap();
        thread::spawn(move || {
            // Daemon side of the handshake: expect Hello, answer Welcome.
            match read_frame::<_, Request>(&mut server_end) {
                Ok(Request::Hello { protocol_version }) if protocol_version == PROTOCOL_VERSION => {
                    write_frame(
                        &mut server_end,
                        &Response::Welcome {
                            protocol_version: PROTOCOL_VERSION,
                        },
                    )
                    .unwrap();
                }
                other => panic!("expected Hello, got {other:?}"),
            }
            script(&mut server_end);
        });
        Client::over(client_end).unwrap()
    }

    #[test]
    fn handshake_then_list_sessions() {
        let mut client = with_daemon(|server| {
            assert!(matches!(
                read_frame::<_, Request>(server).unwrap(),
                Request::ListSessions
            ));
            write_frame(
                server,
                &Response::Sessions(vec![Session {
                    id: "hyprland".to_string(),
                    name: "Hyprland".to_string(),
                    comment: None,
                }]),
            )
            .unwrap();
        });
        let sessions = client.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "hyprland");
    }

    #[test]
    fn auth_conversation_password_then_success() {
        let mut client = with_daemon(|server| {
            assert!(matches!(
                read_frame::<_, Request>(server).unwrap(),
                Request::BeginAuth { .. }
            ));
            write_frame(
                server,
                &Response::Auth(AuthPrompt::Question {
                    text: "Password:".to_string(),
                    secret: true,
                }),
            )
            .unwrap();
            match read_frame::<_, Request>(server).unwrap() {
                Request::AuthReply { response } => assert_eq!(response.expose(), "hunter2"),
                other => panic!("expected AuthReply, got {other:?}"),
            }
            write_frame(server, &Response::AuthSuccess).unwrap();
        });

        client.begin_auth("stephen").unwrap();
        assert_eq!(
            client.recv_auth().unwrap(),
            AuthStep::Question {
                text: "Password:".to_string(),
                secret: true
            }
        );
        client.reply(Secret::new("hunter2".to_string())).unwrap();
        assert_eq!(client.recv_auth().unwrap(), AuthStep::Success);
    }

    #[test]
    fn start_refused_maps_to_refused() {
        let mut client = with_daemon(|server| {
            assert!(matches!(
                read_frame::<_, Request>(server).unwrap(),
                Request::Start { .. }
            ));
            write_frame(
                server,
                &Response::Error {
                    message: "no such session".to_string(),
                },
            )
            .unwrap();
        });
        assert_eq!(
            client.start("ghost").unwrap(),
            StartOutcome::Refused("no such session".to_string())
        );
    }

    #[test]
    fn incompatible_handshake_is_reported() {
        let (client_end, mut server_end) = UnixStream::pair().unwrap();
        thread::spawn(move || {
            let _ = read_frame::<_, Request>(&mut server_end);
            write_frame(
                &mut server_end,
                &Response::Incompatible {
                    daemon_protocol_version: 99,
                },
            )
            .unwrap();
        });
        match Client::over(client_end) {
            Err(ClientError::Incompatible { daemon_version }) => assert_eq!(daemon_version, 99),
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }
}
