//! Length-prefixed JSON framing — the one wire encoding both sides share.
//!
//! Each message is a 4-byte big-endian `u32` length header followed by exactly
//! that many bytes of `serde_json`. Two properties matter for the trust
//! boundary:
//!
//! - **Bounded allocation.** The reader checks the declared length against
//!   [`MAX_FRAME_BYTES`] *before* allocating, so an untrusted greeter cannot
//!   announce a 4 GiB frame and force a giant allocation in the daemon.
//! - **No secret echo.** A malformed frame may contain a mistyped password, so
//!   decode errors carry the error *kind* only ([`FrameError`]) and never the
//!   offending bytes.
//!
//! Both the daemon and the greeter call the same [`read_frame`]/[`write_frame`],
//! so there is exactly one framing implementation to review.

use std::io::{self, Read, Write};

use serde::{de::DeserializeOwned, Serialize};

/// Hard cap on a single serialized message. Login traffic is tiny (a username,
/// a password, a short session list); 64 KiB is comfortably above any honest
/// message and well below anything that threatens the daemon. A frame that
/// claims to exceed it is rejected outright.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Why a frame could not be read or written. Deliberately coarse: it names the
/// failure class without ever embedding frame contents (which may be a secret).
#[derive(Debug)]
pub enum FrameError {
    /// Underlying socket I/O failed (closed connection, timeout, etc.).
    Io(io::Error),
    /// The declared or actual frame size exceeds [`MAX_FRAME_BYTES`].
    TooLarge { declared: usize },
    /// The body was not valid JSON for the expected message type. The bytes are
    /// intentionally dropped — they are not included here.
    Malformed,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Io(e) => write!(f, "frame i/o error: {e}"),
            FrameError::TooLarge { declared } => {
                write!(
                    f,
                    "frame too large: {declared} bytes exceeds {MAX_FRAME_BYTES} cap"
                )
            }
            FrameError::Malformed => {
                f.write_str("malformed frame (not valid for the expected message)")
            }
        }
    }
}

impl std::error::Error for FrameError {}

impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        FrameError::Io(e)
    }
}

/// Serialize `msg` and write it as one length-prefixed frame.
///
/// Returns [`FrameError::TooLarge`] rather than emitting a frame that the peer
/// would reject, so an over-cap message fails fast on the sending side too.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    let body = serde_json::to_vec(msg).map_err(|_| FrameError::Malformed)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            declared: body.len(),
        });
    }
    let len = (body.len() as u32).to_be_bytes();
    w.write_all(&len)?;
    w.write_all(&body)?;
    w.flush()?;
    Ok(())
}

/// Read one length-prefixed frame and decode it into `T`.
///
/// The length is validated against [`MAX_FRAME_BYTES`] before the body buffer is
/// allocated. A decode failure yields [`FrameError::Malformed`] with no payload.
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, FrameError> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let declared = u32::from_be_bytes(len_buf) as usize;
    if declared > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge { declared });
    }
    let mut body = vec![0u8; declared];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|_| FrameError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Request, Response};

    #[test]
    fn round_trips_a_request() {
        let msg = Request::Hello {
            protocol_version: 1,
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        // 4-byte header + body.
        assert!(buf.len() > 4);
        let mut cursor = io::Cursor::new(buf);
        let got: Request = read_frame(&mut cursor).unwrap();
        assert_eq!(got, msg);
    }

    #[test]
    fn rejects_oversized_declared_length_without_allocating() {
        // Header claims 4 GiB; no body follows. Must fail on the cap, not hang
        // trying to read or allocate the body.
        let huge = u32::MAX.to_be_bytes();
        let mut cursor = io::Cursor::new(huge.to_vec());
        let err = read_frame::<_, Request>(&mut cursor).unwrap_err();
        assert!(matches!(err, FrameError::TooLarge { .. }));
    }

    #[test]
    fn malformed_body_does_not_echo_bytes() {
        // Valid framing, body is not a Request.
        let body = b"{\"NotAVariant\":true}";
        let mut buf = (body.len() as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(body);
        let mut cursor = io::Cursor::new(buf);
        let err = read_frame::<_, Request>(&mut cursor).unwrap_err();
        assert!(matches!(err, FrameError::Malformed));
        // The error's text never contains the frame payload.
        assert!(!format!("{err}").contains("NotAVariant"));
    }

    #[test]
    fn responses_frame_too() {
        let msg = Response::Welcome {
            protocol_version: 1,
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let mut cursor = io::Cursor::new(buf);
        let got: Response = read_frame(&mut cursor).unwrap();
        assert_eq!(got, msg);
    }
}
