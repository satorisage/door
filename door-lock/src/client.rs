//! The lock screen's client end of the reauth seam.
//!
//! A thin, synchronous wrapper over the daemon's **reauth socket** — the
//! dedicated, world-connectable socket whose only vocabulary is "verify the
//! connecting peer's own credentials." It dials `DOORD_REAUTH_SOCKET`, runs the
//! mandatory version handshake, and drives the verify-only conversation prompt
//! by prompt. It carries **no identity**: the daemon binds the reauth target to
//! this process's kernel-attested `SO_PEERCRED` uid, so this client can only
//! ever ask "am I who I say I am?" — never probe another account.
//!
//! Blocking I/O lives here on purpose, mirroring the greeter's client: the UI
//! runs it on a background worker, and the synchronous shape unit-tests cleanly
//! against a scripted daemon over a socket pair. The daemon drops peers that go
//! idle between attempts, so the worker dials a **fresh connection per unlock
//! attempt** rather than holding one open across the (unbounded) locked wait.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;

use protocol::{
    read_frame, write_frame, AuthPrompt, FrameError, ReauthRequest, ReauthResponse, Secret,
    PROTOCOL_VERSION,
};

/// Default production reauth socket — matches the daemon's
/// `DEFAULT_REAUTH_SOCKET_PATH`.
pub const DEFAULT_REAUTH_SOCKET: &str = "/run/doord-reauth/reauth.sock";

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
    Unexpected(ReauthResponse),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Io(e) => write!(f, "socket error: {e}"),
            ClientError::Frame(e) => write!(f, "protocol framing error: {e}"),
            ClientError::Incompatible { daemon_version } => write!(
                f,
                "the daemon speaks protocol v{daemon_version}; this locker speaks v{PROTOCOL_VERSION}"
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

/// One step the daemon surfaces during a reauthentication conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReauthStep {
    /// PAM wants input — render `text`; mask it if `secret`. Answer with
    /// [`Client::reply`].
    Question { text: String, secret: bool },
    /// A one-way informational message to show the user.
    Info(String),
    /// A one-way error message to show the user.
    Error(String),
    /// Reauthentication succeeded: this uid's credentials verified. Unlock.
    Allow,
    /// Reauthentication failed (the reason is deliberately coarse); retry.
    Deny(String),
}

/// A connected, handshaken reauth client.
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
        self.send(&ReauthRequest::Hello {
            protocol_version: PROTOCOL_VERSION,
        })?;
        match self.recv()? {
            ReauthResponse::Welcome { .. } => Ok(()),
            ReauthResponse::Incompatible {
                daemon_protocol_version,
            } => Err(ClientError::Incompatible {
                daemon_version: daemon_protocol_version,
            }),
            other => Err(ClientError::Unexpected(other)),
        }
    }

    /// Begin a verify-only reauthentication of this process's own uid. Carries no
    /// identity — the daemon derives the target from the connection's peer cred.
    /// Follow with [`recv_step`](Client::recv_step) to read the daemon's prompts.
    pub fn begin(&mut self) -> Result<()> {
        self.send(&ReauthRequest::Begin)
    }

    /// Read the next conversation step from the daemon.
    pub fn recv_step(&mut self) -> Result<ReauthStep> {
        match self.recv()? {
            ReauthResponse::Prompt(AuthPrompt::Question { text, secret }) => {
                Ok(ReauthStep::Question { text, secret })
            }
            ReauthResponse::Prompt(AuthPrompt::Info { text }) => Ok(ReauthStep::Info(text)),
            ReauthResponse::Prompt(AuthPrompt::Error { text }) => Ok(ReauthStep::Error(text)),
            ReauthResponse::Allow => Ok(ReauthStep::Allow),
            ReauthResponse::Deny { reason } => Ok(ReauthStep::Deny(reason)),
            other => Err(ClientError::Unexpected(other)),
        }
    }

    /// Answer the most recent [`ReauthStep::Question`]. The secret is moved in and
    /// dropped (zeroized) once written.
    pub fn reply(&mut self, response: Secret) -> Result<()> {
        self.send(&ReauthRequest::Reply { response })
    }

    /// Abandon the in-progress reauthentication. Part of the complete protocol
    /// surface; wired to a UI affordance when the locker grows a cancel button.
    #[allow(dead_code)]
    pub fn cancel(&mut self) -> Result<()> {
        self.send(&ReauthRequest::Cancel)
    }

    fn send(&mut self, request: &ReauthRequest) -> Result<()> {
        write_frame(&mut self.stream, request)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<ReauthResponse> {
        Ok(read_frame::<_, ReauthResponse>(&mut self.stream)?)
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
            match read_frame::<_, ReauthRequest>(&mut server_end) {
                Ok(ReauthRequest::Hello { protocol_version })
                    if protocol_version == PROTOCOL_VERSION =>
                {
                    write_frame(
                        &mut server_end,
                        &ReauthResponse::Welcome {
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
    fn conversation_password_then_allow() {
        let mut client = with_daemon(|server| {
            assert!(matches!(
                read_frame::<_, ReauthRequest>(server).unwrap(),
                ReauthRequest::Begin
            ));
            write_frame(
                server,
                &ReauthResponse::Prompt(AuthPrompt::Question {
                    text: "Password:".to_string(),
                    secret: true,
                }),
            )
            .unwrap();
            match read_frame::<_, ReauthRequest>(server).unwrap() {
                ReauthRequest::Reply { response } => assert_eq!(response.expose(), "hunter2"),
                other => panic!("expected Reply, got {other:?}"),
            }
            write_frame(server, &ReauthResponse::Allow).unwrap();
        });

        client.begin().unwrap();
        assert_eq!(
            client.recv_step().unwrap(),
            ReauthStep::Question {
                text: "Password:".to_string(),
                secret: true
            }
        );
        client.reply(Secret::new("hunter2".to_string())).unwrap();
        assert_eq!(client.recv_step().unwrap(), ReauthStep::Allow);
    }

    #[test]
    fn deny_maps_to_deny_and_the_client_can_begin_again() {
        let mut client = with_daemon(|server| {
            for _ in 0..2 {
                assert!(matches!(
                    read_frame::<_, ReauthRequest>(server).unwrap(),
                    ReauthRequest::Begin
                ));
                write_frame(
                    server,
                    &ReauthResponse::Deny {
                        reason: "Authentication failed".to_string(),
                    },
                )
                .unwrap();
            }
        });
        // Two attempts over one connection: the daemon serves Begin repeatedly.
        for _ in 0..2 {
            client.begin().unwrap();
            assert_eq!(
                client.recv_step().unwrap(),
                ReauthStep::Deny("Authentication failed".to_string())
            );
        }
    }

    #[test]
    fn incompatible_handshake_is_reported() {
        let (client_end, mut server_end) = UnixStream::pair().unwrap();
        thread::spawn(move || {
            let _ = read_frame::<_, ReauthRequest>(&mut server_end);
            write_frame(
                &mut server_end,
                &ReauthResponse::Incompatible {
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

    /// The wire shape of `Begin` is fieldless — there is structurally no place to
    /// put a username, so a locker cannot be turned into a cross-user oracle even
    /// by patching this client. Guard the shape.
    #[test]
    fn begin_carries_no_identity_on_the_wire() {
        let bytes = serde_json::to_vec(&ReauthRequest::Begin).unwrap();
        assert_eq!(bytes, b"\"Begin\"");
    }
}
