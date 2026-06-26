//! The door wire protocol — the single seam between the unprivileged greeter
//! and the privileged daemon.
//!
//! This crate is the trust boundary made explicit. The greeter is an
//! unprivileged process: it collects what the user types and asks the daemon to
//! act on it, but it never authenticates anyone, never owns a seat, and never
//! starts a session itself. Every privileged action is a [`Request`] the daemon
//! is free to refuse. The daemon must treat everything arriving over this seam
//! as untrusted input — a compromised greeter can send any bytes it likes.
//!
//! The protocol is deliberately door's own (not borrowed from any other login
//! manager) so it can model door's richer flows: multi-prompt PAM
//! conversations, session metadata for a nicer picker, and presentation hints
//! the daemon may emit without ever handing the greeter authority.

use serde::{Deserialize, Serialize};

/// A desktop session the daemon discovered and is willing to start.
///
/// Sourced from the freedesktop session directories; the `exec` is what the
/// daemon will run on a successful login (honoring wrapper launchers). The
/// greeter renders `name`/`comment` only — it never runs `exec` itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Stable identifier (the `.desktop` basename), used to select on `Start`.
    pub id: String,
    /// Human-facing name for the picker.
    pub name: String,
    /// Optional one-line description.
    pub comment: Option<String>,
}

/// One step in an authentication conversation, surfaced by PAM via the daemon.
///
/// PAM drives the dialogue: it may ask for a password, a second factor, or show
/// an informational message. The greeter renders the prompt and returns the
/// user's reply in an [`Request::AuthReply`]. `secret` marks input that must be
/// masked and never echoed or logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthPrompt {
    /// Ask the user for input. `secret` ⇒ mask it (password, OTP).
    Question { text: String, secret: bool },
    /// Show the user a message; no reply expected.
    Info { text: String },
    /// Show the user an error message; no reply expected.
    Error { text: String },
}

/// Greeter → daemon. Requests for privileged action; the daemon may refuse any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Ask for the list of startable sessions.
    ListSessions,
    /// Begin an authentication conversation for `username`.
    BeginAuth { username: String },
    /// Answer the most recent [`AuthPrompt::Question`]. The reply may be a
    /// secret; it lives only as long as the daemon's PAM call needs it and is
    /// zeroized after.
    AuthReply { response: String },
    /// Abandon the in-progress conversation and start over.
    CancelAuth,
    /// After a successful auth, start `session_id` for the authenticated user.
    Start { session_id: String },
    /// Power controls — the daemon performs them, the greeter only asks.
    Power(PowerAction),
}

/// Seat-level power actions the greeter may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerAction {
    Reboot,
    PowerOff,
    Suspend,
}

/// Daemon → greeter. State of the conversation and answers to requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    /// The startable sessions (answer to [`Request::ListSessions`]).
    Sessions(Vec<Session>),
    /// PAM needs the greeter to render this prompt and (if a question) reply.
    Auth(AuthPrompt),
    /// Authentication succeeded; the greeter may now [`Request::Start`].
    AuthSuccess,
    /// Authentication failed; the conversation is over (greeter may retry).
    AuthFailure { reason: String },
    /// The daemon refused or could not satisfy a request. Never leaks
    /// privileged detail; it is shown to a user standing at the login screen.
    Error { message: String },
}
