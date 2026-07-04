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
//! The protocol is deliberately door's own — door is replacing the login layer,
//! not joining an existing one, so the seam is free to model door's richer
//! flows: multi-prompt PAM conversations, session metadata for a nicer picker,
//! and presentation hints the daemon may emit without ever handing the greeter
//! authority.
//!
//! # Security & evolution invariants (the seam's contract)
//!
//! - **Versioned before credentialed.** The greeter's first message is always
//!   [`Request::Hello`]; no credential crosses the seam until the daemon has
//!   answered [`Response::Welcome`]. See [`PROTOCOL_VERSION`].
//! - **Strict in, lenient out.** Greeter→daemon types carry
//!   `#[serde(deny_unknown_fields)]`: untrusted input to the trusted core is
//!   parsed strictly and rejects anything unexpected. Daemon→greeter types stay
//!   forward-lenient, so an older greeter degrades gracefully against a newer
//!   daemon instead of failing closed.
//! - **Additive evolution only.** New flows are new enum variants guarded by a
//!   [`PROTOCOL_VERSION`] bump — never untyped maps, flattening, or relaxed
//!   parsing, which would re-open the boundary.
//! - **Secrets are typed.** Credentials travel as [`Secret`], which never
//!   renders in `Debug` and is zeroized on drop.
//! - **Bounded framing.** Messages cross as length-prefixed JSON with a hard
//!   size cap; see [`frame`].

use serde::{Deserialize, Serialize};

pub mod frame;
mod secret;

pub use frame::{read_frame, write_frame, FrameError, MAX_FRAME_BYTES};
pub use secret::Secret;

/// The wire-protocol version this build speaks.
///
/// Negotiated by the [`Request::Hello`]/[`Response::Welcome`] handshake before
/// any credential crosses the seam. Bump this on **any** change to message
/// shape (new variant, new field, changed encoding); the handshake then keeps a
/// peer from ever being handed a message its version cannot parse.
///
/// v2 added [`Response::Started`] — the session-spawn outcome — as an additive
/// variant (the sanctioned evolution path: a new variant guarded by this bump).
///
/// v3 added the **reauth seam** — [`ReauthRequest`]/[`ReauthResponse`], a verify-only
/// credential-check vocabulary spoken on a *separate* socket by the session lock
/// screen. Additive: new variants, guarded by this bump. Deliberately its own
/// enum pair rather than more `Request`/`Response` variants, so the reauth seam has
/// no session-spawn verb in its type system at all (its whole point is to verify and
/// report, never to start a session).
pub const PROTOCOL_VERSION: u32 = 3;

/// A desktop session the daemon discovered and is willing to start.
///
/// Sourced from the freedesktop session directories; the daemon runs the real
/// `Exec=` (honoring wrapper launchers) on a successful login. The greeter
/// renders `name`/`comment` only — it never runs anything itself.
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
/// user's reply in a [`Request::AuthReply`]. `secret` marks input that must be
/// masked and never echoed or logged.
///
/// Daemon→greeter, so forward-lenient: a future PAM prompt style is a new
/// variant a newer daemon only emits once the handshake proves the greeter
/// understands it.
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
///
/// **Untrusted input to the TCB** — hence `deny_unknown_fields`: a stray or
/// extra field is a malformed frame and is rejected, not ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Request {
    /// Always the first message. Declares the version the greeter speaks; the
    /// daemon answers [`Response::Welcome`] or [`Response::Incompatible`]. No
    /// other request is honored before a successful handshake.
    Hello { protocol_version: u32 },
    /// Ask for the list of startable sessions.
    ListSessions,
    /// Begin an authentication conversation for `username`.
    BeginAuth { username: String },
    /// Answer the most recent [`AuthPrompt::Question`]. The reply is a
    /// [`Secret`]: it lives only as long as the daemon's PAM call needs it and
    /// is zeroized after.
    AuthReply { response: Secret },
    /// Abandon the in-progress conversation and start over.
    CancelAuth,
    /// After a successful auth, start `session_id` for the authenticated user.
    Start { session_id: String },
    /// Power controls — the daemon performs them, the greeter only asks.
    Power(PowerAction),
}

/// Seat-level power actions the greeter may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PowerAction {
    Reboot,
    PowerOff,
    Suspend,
}

/// Daemon → greeter. State of the conversation and answers to requests.
///
/// Forward-lenient by design (no `deny_unknown_fields`): a newer daemon may add
/// fields an older greeter ignores. New variants are version-gated by the
/// handshake, so an older greeter is never sent one it cannot parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    /// Handshake accepted; the daemon speaks `protocol_version`. The greeter may
    /// now proceed. The minimum of the two versions is the agreed dialect.
    Welcome { protocol_version: u32 },
    /// Handshake rejected: the daemon cannot speak the greeter's version. The
    /// daemon closes the connection after sending this; the greeter shows why.
    Incompatible { daemon_protocol_version: u32 },
    /// The startable sessions (answer to [`Request::ListSessions`]).
    Sessions(Vec<Session>),
    /// PAM needs the greeter to render this prompt and (if a question) reply.
    Auth(AuthPrompt),
    /// Authentication succeeded; the greeter may now [`Request::Start`].
    AuthSuccess,
    /// The chosen session was launched for the authenticated user (answer to a
    /// successful [`Request::Start`]). The greeter should step aside: the daemon
    /// now owns the running session. Carries no detail — the session's pid, seat,
    /// and lifecycle live entirely on the privileged side of the seam.
    Started,
    /// Authentication failed; the conversation is over (greeter may retry).
    AuthFailure { reason: String },
    /// The daemon refused or could not satisfy a request. Never leaks
    /// privileged detail; it is shown to a user standing at the login screen.
    Error { message: String },
}

/// Session-lock client → daemon, on the **reauth seam** (a separate socket from the
/// greeter's, spoken by the lock screen). Asks the daemon to reauthenticate the
/// *connecting peer's own uid* — verify-only, never a session spawn.
///
/// **Untrusted input to the TCB** — `deny_unknown_fields`, exactly like [`Request`]:
/// an extra or stray field is a malformed frame and is rejected.
///
/// This vocabulary has no session verb by design: the reauth seam can verify a
/// credential and nothing else, so "no session ever" is a property of the type, not
/// of a handler that happens to decline. Note especially that [`ReauthRequest::Begin`]
/// carries **no username**: the daemon derives the reauth target uid solely from the
/// connection's `SO_PEERCRED`, so a client can only ever reauthenticate as *itself*
/// (no cross-user brute-force oracle). A client that tries to smuggle a target
/// identity — e.g. an extra `username` field — is rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ReauthRequest {
    /// Always the first message. Declares the version the client speaks; the daemon
    /// answers [`ReauthResponse::Welcome`] or [`ReauthResponse::Incompatible`]. No
    /// other reauth request is honored before a successful handshake.
    Hello { protocol_version: u32 },
    /// Begin a verify-only reauthentication of this connection's own uid. Carries no
    /// identity — the target uid is the kernel-attested `SO_PEERCRED` uid.
    Begin,
    /// Answer the most recent [`AuthPrompt::Question`]. The reply is a [`Secret`]: it
    /// lives only as long as the daemon's PAM call needs it and is zeroized after.
    Reply { response: Secret },
    /// Abandon the in-progress reauthentication.
    Cancel,
}

/// Daemon → session-lock client, on the **reauth seam**. Carries the reauth
/// conversation and its verdict.
///
/// Forward-lenient by design (no `deny_unknown_fields`), like [`Response`]: a newer
/// daemon may add fields an older client ignores; new variants are version-gated by
/// the handshake. There is deliberately no "started"/session variant — a reauth
/// allow is just an allow, with no seat, pid, or session behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReauthResponse {
    /// Handshake accepted; the daemon speaks `protocol_version`.
    Welcome { protocol_version: u32 },
    /// Handshake rejected: the daemon cannot speak the client's version. The daemon
    /// closes the connection after sending this.
    Incompatible { daemon_protocol_version: u32 },
    /// PAM needs the client to render this prompt and (if a question) reply. Reuses
    /// the greeter's [`AuthPrompt`] so a lock screen renders identically to the login
    /// screen.
    Prompt(AuthPrompt),
    /// Reauthentication succeeded: the peer's own credentials verified. The client may
    /// unlock. Carries nothing — there is no session, seat, or spawn on this seam.
    Allow,
    /// Reauthentication failed or was cancelled. The reason is deliberately coarse: it
    /// never reveals whether the account exists or which factor failed.
    Deny { reason: String },
    /// The daemon could not process the request (malformed, or sent before the
    /// handshake). Coarse; never leaks privileged detail.
    Error { message: String },
}
