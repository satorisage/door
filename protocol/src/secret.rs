//! A typed credential that refuses to leak.
//!
//! Everything the user types into a masked field is a [`Secret`], not a bare
//! `String`. That buys three guarantees the rest of the seam relies on:
//!
//! - its `Debug` renders `Secret(<redacted>)`, so logging a whole [`crate::Request`]
//!   can never spill a password into the journal;
//! - its bytes are zeroized when it drops, so a credential does not linger in
//!   freed heap memory after the PAM call consumes it;
//! - reading the plaintext is an explicit [`Secret::expose`] call, which makes
//!   every place a secret is actually read greppable and reviewable.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A secret string (password, OTP) carried greeter→daemon for one PAM step.
///
/// Serializes transparently as the underlying string so it is wire-compatible
/// with a plain field, but in Rust it is opaque: no `Debug` leak, zeroized on
/// drop. Hold the [`expose`](Secret::expose) borrow for as short a window as
/// possible — ideally only across the PAM conversation call.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wrap a freshly collected secret.
    pub fn new(value: String) -> Self {
        Secret(value)
    }

    /// Borrow the plaintext. Every call site is intentionally visible in review:
    /// the secret should be exposed only to hand it to PAM, never to log or copy.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Equality is provided for message-level comparison (tests, idempotence
/// checks) only — it is **not** an authentication primitive and is not
/// constant-time. Never use `==` on a `Secret` to verify a credential.
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Secret {}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn debug_is_redacted() {
        let s = Secret::new("hunter2".to_string());
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert!(!format!("{s:?}").contains("hunter2"));
    }

    #[test]
    fn serializes_transparently_as_a_string() {
        let s = Secret::new("hunter2".to_string());
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"hunter2\"");
        let back: Secret = serde_json::from_str("\"hunter2\"").unwrap();
        assert_eq!(back.expose(), "hunter2");
    }
}
