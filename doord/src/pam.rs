//! Authentication: the PAM conversation, bridged to the greeter over IPC.
//!
//! PAM drives the dialogue — it decides when to ask for a password, a second
//! factor, or show a message — but door's greeter is what the user actually
//! types into, across the socket. This module is the bridge: each PAM prompt is
//! turned into a [`protocol::AuthPrompt`] sent to the greeter, and the greeter's
//! reply is fed back to PAM.
//!
//! The bridge is split from PAM itself by two small traits so the IPC flow can
//! be tested without root or a live PAM stack:
//!
//! - [`AuthChannel`] is "how to ask the greeter something" — the production
//!   impl ([`SocketChannel`]) writes/reads frames; a test impl can script it.
//! - [`Authenticator`] is "what decides the outcome" — the production impl
//!   ([`PamAuthenticator`]) runs PAM; a test impl can return a fixed verdict.
//!
//! Secrets are handled with care end to end: the greeter's reply arrives as a
//! [`Secret`] (redacted, zeroized) and is only widened to the `CString` PAM
//! requires at the last moment, inside the conversation callback.

use std::ffi::{CStr, CString};
use std::os::unix::net::UnixStream;

use pam_client::{Context, ConversationHandler, ErrorCode, Flag};
use protocol::{read_frame, write_frame, AuthPrompt, Request, Response, Secret};

/// The result of an authentication attempt, as the daemon will report it.
#[derive(Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Credentials accepted and the account is permitted to log in.
    Success,
    /// Authentication or account checks rejected the attempt. The reason is
    /// deliberately coarse here — the detailed PAM error is journaled, never
    /// sent to the login screen (it could reveal whether an account exists).
    Failure,
    /// The user asked to cancel the in-progress conversation.
    Cancelled,
    /// The greeter connection broke mid-conversation; the caller should drop it.
    Transport,
}

/// Why an [`AuthChannel`] round-trip could not complete.
#[derive(Debug)]
pub enum ChannelFault {
    /// The greeter sent [`Request::CancelAuth`] instead of a reply.
    Cancelled,
    /// The socket failed (closed, timed out).
    Transport,
    /// The greeter sent something other than a reply or a cancel.
    Protocol,
}

/// How the daemon talks to the greeter during a conversation. Abstracted so the
/// auth flow can be driven by a scripted channel in tests.
pub trait AuthChannel {
    /// Ask the greeter a question and wait for the typed reply.
    fn ask(&mut self, text: &str, secret: bool) -> Result<Secret, ChannelFault>;
    /// Show the greeter a one-way message (no reply expected). Best-effort: a
    /// transport failure here is reported via the next [`ask`](AuthChannel::ask)
    /// or surfaces as the conversation ending.
    fn notify(&mut self, prompt: AuthPrompt);
}

/// What decides an authentication outcome. The production impl runs PAM; tests
/// can substitute a scripted verdict to exercise the IPC flow deterministically.
pub trait Authenticator {
    fn authenticate(&self, username: &str, channel: &mut dyn AuthChannel) -> AuthOutcome;
}

/// Production [`AuthChannel`]: each prompt is a frame to the greeter, each reply
/// a frame back.
pub struct SocketChannel<'a> {
    stream: &'a mut UnixStream,
}

impl<'a> SocketChannel<'a> {
    pub fn new(stream: &'a mut UnixStream) -> Self {
        SocketChannel { stream }
    }
}

impl AuthChannel for SocketChannel<'_> {
    fn ask(&mut self, text: &str, secret: bool) -> Result<Secret, ChannelFault> {
        let prompt = Response::Auth(AuthPrompt::Question {
            text: text.to_string(),
            secret,
        });
        if write_frame(self.stream, &prompt).is_err() {
            return Err(ChannelFault::Transport);
        }
        match read_frame::<_, Request>(self.stream) {
            Ok(Request::AuthReply { response }) => Ok(response),
            Ok(Request::CancelAuth) => Err(ChannelFault::Cancelled),
            Ok(_) => Err(ChannelFault::Protocol),
            Err(_) => Err(ChannelFault::Transport),
        }
    }

    fn notify(&mut self, prompt: AuthPrompt) {
        // One-way; a failure just means the next ask() will also fail and end
        // the conversation. We intentionally don't surface it here.
        let _ = write_frame(self.stream, &Response::Auth(prompt));
    }
}

/// Production [`Authenticator`]: runs a real PAM transaction for `service`.
pub struct PamAuthenticator {
    service: String,
}

impl PamAuthenticator {
    pub fn new(service: impl Into<String>) -> Self {
        PamAuthenticator {
            service: service.into(),
        }
    }
}

impl Authenticator for PamAuthenticator {
    fn authenticate(&self, username: &str, channel: &mut dyn AuthChannel) -> AuthOutcome {
        let conv = GreeterConversation {
            channel,
            fault: None,
        };
        let mut context = match Context::new(&self.service, Some(username), conv) {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("doord: pam_start failed for service '{}': {e}", self.service);
                return AuthOutcome::Failure;
            }
        };

        let auth_result = context.authenticate(Flag::NONE);

        // A transport break or cancel during the conversation takes precedence
        // over whatever code PAM returned for the aborted attempt.
        if let Some(fault) = context.conversation_mut().fault.take() {
            return match fault {
                ChannelFault::Cancelled => AuthOutcome::Cancelled,
                ChannelFault::Transport => AuthOutcome::Transport,
                ChannelFault::Protocol => AuthOutcome::Failure,
            };
        }

        if let Err(e) = auth_result {
            // Detailed reason to the journal only — never to the greeter.
            eprintln!("doord: authentication failed for '{username}': {e}");
            return AuthOutcome::Failure;
        }

        // Authenticated — now check the account is allowed to log in at all
        // (not expired, not locked, password not required-to-change-and-absent).
        if let Err(e) = context.acct_mgmt(Flag::NONE) {
            eprintln!("doord: account check denied '{username}': {e}");
            return AuthOutcome::Failure;
        }

        AuthOutcome::Success
    }
}

/// The PAM-facing conversation callback. libpam calls these methods; each one
/// relays through the [`AuthChannel`] to the greeter. A `fault` is latched the
/// first time a round-trip fails so the outer [`PamAuthenticator`] can tell a
/// dropped greeter from a real authentication failure.
struct GreeterConversation<'a> {
    channel: &'a mut dyn AuthChannel,
    fault: Option<ChannelFault>,
}

impl GreeterConversation<'_> {
    fn ask(&mut self, prompt: &CStr, secret: bool) -> Result<CString, ErrorCode> {
        if self.fault.is_some() {
            return Err(ErrorCode::CONV_ERR);
        }
        let text = prompt.to_string_lossy();
        match self.channel.ask(&text, secret) {
            Ok(reply) => {
                // Widen to the CString PAM needs only here, at the last moment.
                // A NUL inside the secret can't be a valid password; reject it.
                let result = CString::new(reply.expose()).map_err(|_| ErrorCode::CONV_ERR);
                // `reply` (a Secret) is zeroized as it drops at end of scope.
                result
            }
            Err(fault) => {
                self.fault = Some(fault);
                Err(ErrorCode::CONV_ERR)
            }
        }
    }
}

impl ConversationHandler for GreeterConversation<'_> {
    fn prompt_echo_on(&mut self, prompt: &CStr) -> Result<CString, ErrorCode> {
        self.ask(prompt, false)
    }

    fn prompt_echo_off(&mut self, prompt: &CStr) -> Result<CString, ErrorCode> {
        self.ask(prompt, true)
    }

    fn text_info(&mut self, msg: &CStr) {
        if self.fault.is_some() {
            return;
        }
        self.channel.notify(AuthPrompt::Info {
            text: msg.to_string_lossy().into_owned(),
        });
    }

    fn error_msg(&mut self, msg: &CStr) {
        if self.fault.is_some() {
            return;
        }
        self.channel.notify(AuthPrompt::Error {
            text: msg.to_string_lossy().into_owned(),
        });
    }
}

/// A scripted authenticator for tests: asks once for a secret and accepts only
/// if it matches `password`. Lets the IPC auth flow be exercised end to end
/// without root or a live PAM stack.
#[cfg(test)]
pub struct ScriptedAuthenticator {
    pub password: String,
}

#[cfg(test)]
impl Authenticator for ScriptedAuthenticator {
    fn authenticate(&self, _username: &str, channel: &mut dyn AuthChannel) -> AuthOutcome {
        match channel.ask("Password:", true) {
            Ok(reply) if reply.expose() == self.password => AuthOutcome::Success,
            Ok(_) => AuthOutcome::Failure,
            Err(ChannelFault::Cancelled) => AuthOutcome::Cancelled,
            Err(_) => AuthOutcome::Transport,
        }
    }
}
