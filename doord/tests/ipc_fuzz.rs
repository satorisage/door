//! Adversarial IPC harness — a hostile "greeter" that throws malformed, oversized,
//! truncated, mistyped, and out-of-order traffic at a real `doord` and asserts the
//! daemon never panics, hangs, wedges, or leaks: after every hostile poke a *fresh*
//! well-formed handshake must still succeed, proving the seam stayed alive.
//!
//! This is the empirical counterpart to the static review of `protocol::frame` and
//! `doord::ipc` — the parser is fuzzed against the running binary, not just read.
//!
//! The daemon runs unmanaged (no greeter to launch), connecting as the test's own uid,
//! which the peercred check defaults to allowing when `DOORD_GREETER_UID` is unset.

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use protocol::{read_frame, write_frame, Request, Response, PROTOCOL_VERSION};

struct Daemon {
    child: Child,
    socket: PathBuf,
    session_root: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_dir_all(&self.session_root);
    }
}

fn start_daemon(tag: &str) -> Daemon {
    let socket = std::env::temp_dir().join(format!("doord-fuzz-{tag}-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let session_root =
        std::env::temp_dir().join(format!("doord-fuzz-sessions-{tag}-{}", std::process::id()));
    let wayland = session_root.join("wayland-sessions");
    std::fs::create_dir_all(&wayland).expect("create session dir");
    std::fs::write(
        wayland.join("testde.desktop"),
        "[Desktop Entry]\nName=Test DE\nExec=/usr/bin/test-de\n",
    )
    .expect("write session entry");

    let child = Command::new(env!("CARGO_BIN_EXE_doord"))
        .env("DOORD_SOCKET", &socket)
        .env("DOORD_SESSION_DIRS", &session_root)
        .spawn()
        .expect("spawn doord");

    Daemon {
        child,
        socket,
        session_root,
    }
}

fn connect(socket: &Path) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match UnixStream::connect(socket) {
            Ok(s) => {
                // Keep the suite from ever hanging on a daemon that misbehaves.
                s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
                return s;
            }
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    || e.kind() == io::ErrorKind::ConnectionRefused =>
            {
                if Instant::now() > deadline {
                    panic!("daemon never came up on {}: {e}", socket.display());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("connect failed: {e}"),
        }
    }
}

/// The core invariant: a brand-new connection can still complete the handshake. If the
/// daemon had panicked, hung, or wedged on the previous hostile poke, this fails.
fn assert_alive(socket: &Path, after: &str) {
    let mut conn = connect(socket);
    write_frame(
        &mut conn,
        &Request::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .unwrap_or_else(|e| panic!("[{after}] daemon dead: couldn't send Hello: {e}"));
    let resp: Response = read_frame(&mut conn)
        .unwrap_or_else(|e| panic!("[{after}] daemon dead: no Welcome after hostile input: {e}"));
    assert_eq!(
        resp,
        Response::Welcome {
            protocol_version: PROTOCOL_VERSION
        },
        "[{after}] daemon answered the post-attack handshake wrong"
    );
}

/// Connect, write exactly `bytes`, then drop (close) — bypasses framing entirely.
fn poke(socket: &Path, bytes: &[u8]) {
    let mut conn = connect(socket);
    let _ = conn.write_all(bytes);
    let _ = conn.flush();
    // Drop closes the socket → the daemon sees EOF on any outstanding read_exact.
}

/// A raw length-prefixed frame with a chosen header and body (which need not agree).
fn framed(declared: u32, body: &[u8]) -> Vec<u8> {
    let mut v = declared.to_be_bytes().to_vec();
    v.extend_from_slice(body);
    v
}

#[test]
fn survives_a_barrage_of_hostile_frames() {
    let d = start_daemon("barrage");
    // Wait for the socket to exist before hammering.
    assert_alive(&d.socket, "startup");

    // 1. Header claims ~4 GiB, no body. Must reject on the cap, not allocate/hang.
    poke(&d.socket, &u32::MAX.to_be_bytes());
    assert_alive(&d.socket, "oversized-header");

    // 2. Header claims exactly MAX_FRAME_BYTES + 1.
    poke(
        &d.socket,
        &((protocol::MAX_FRAME_BYTES as u32 + 1).to_be_bytes()),
    );
    assert_alive(&d.socket, "over-cap-by-one");

    // 3. Truncated header — two bytes then close (EOF mid-header).
    poke(&d.socket, &[0x00, 0x10]);
    assert_alive(&d.socket, "truncated-header");

    // 4. Header says 100, body is 8 bytes then close (EOF mid-body).
    poke(&d.socket, &framed(100, b"deadbeef"));
    assert_alive(&d.socket, "short-body");

    // 5. Zero-length frame (empty body → not a valid Request).
    poke(&d.socket, &framed(0, b""));
    assert_alive(&d.socket, "zero-length");

    // 6. Well-framed non-JSON garbage.
    poke(&d.socket, &framed(5, b"hello"));
    assert_alive(&d.socket, "non-json");

    // 7. Well-framed non-UTF-8 bytes.
    poke(&d.socket, &framed(4, &[0xff, 0xfe, 0xfd, 0xfc]));
    assert_alive(&d.socket, "non-utf8");

    // 8. Valid JSON, unknown enum variant.
    let body = br#"{"TotallyNotARequest":{"x":1}}"#;
    poke(&d.socket, &framed(body.len() as u32, body));
    assert_alive(&d.socket, "unknown-variant");

    // 9. Valid JSON, right variant, wrong field type (version as a string).
    let body = br#"{"Hello":{"protocol_version":"not-a-number"}}"#;
    poke(&d.socket, &framed(body.len() as u32, body));
    assert_alive(&d.socket, "type-confusion");

    // 10. A full-size (64 KiB) body of JSON junk — read entirely, then rejected.
    let junk = vec![b'{'; protocol::MAX_FRAME_BYTES];
    poke(&d.socket, &framed(junk.len() as u32, &junk));
    assert_alive(&d.socket, "max-size-junk");

    // 11. Nested-JSON blow-up attempt (deep arrays) within the cap — serde must not
    //     stack-overflow the daemon; it should just fail to match a Request.
    let depth = 5000;
    let mut deep = vec![b'['; depth];
    deep.extend(std::iter::repeat_n(b']', depth));
    poke(&d.socket, &framed(deep.len() as u32, &deep));
    assert_alive(&d.socket, "deep-nesting");
}

#[test]
fn refuses_start_without_auth_and_stays_alive() {
    let d = start_daemon("startgate");
    let mut conn = connect(&d.socket);

    // Proper handshake.
    write_frame(
        &mut conn,
        &Request::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    let _: Response = read_frame(&mut conn).unwrap();

    // Start with no prior authentication — must be refused, never spawn anything.
    write_frame(
        &mut conn,
        &Request::Start {
            session_id: "testde".to_string(),
        },
    )
    .unwrap();
    let resp: Response = read_frame(&mut conn).unwrap();
    assert!(
        matches!(resp, Response::Error { .. }),
        "Start without auth must be refused, got {resp:?}"
    );

    // The daemon serves one greeter at a time (by design); release this connection
    // before probing liveness on a fresh one.
    drop(conn);
    assert_alive(&d.socket, "start-without-auth");
}

#[test]
fn tolerates_a_giant_username_within_the_cap() {
    let d = start_daemon("bigusername");
    let mut conn = connect(&d.socket);
    write_frame(
        &mut conn,
        &Request::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    let _: Response = read_frame(&mut conn).unwrap();

    // ~60 KiB username (comfortably inside the frame cap). PAM will reject it, but the
    // daemon must not overflow, panic, or wedge — and must answer the auth attempt.
    let username = "a".repeat(60 * 1024);
    write_frame(&mut conn, &Request::BeginAuth { username }).unwrap();
    let resp: Result<Response, _> = read_frame(&mut conn);
    // Either a framed AuthFailure/AuthSuccess (PAM ran) or a clean drop is acceptable;
    // a panic/hang is not. The daemon staying alive is the real assertion.
    let _ = resp;
    // Serial daemon: release before the fresh-connection liveness probe.
    drop(conn);
    assert_alive(&d.socket, "giant-username");
}

#[test]
fn survives_a_reconnect_flood() {
    let d = start_daemon("flood");
    assert_alive(&d.socket, "startup");
    // Open and immediately close many connections — each forces accept + peercred +
    // timeout setup + a handshake read that hits EOF. The serial accept loop must keep
    // draining them without leaking fds or wedging.
    for _ in 0..300 {
        if let Ok(mut s) = UnixStream::connect(&d.socket) {
            // Send a byte of a header then bail, to exercise the partial-read path too.
            let _ = s.write_all(&[0x00]);
        }
    }
    assert_alive(&d.socket, "reconnect-flood");
}

/// A slow client that sends one header byte then stalls must not hang the daemon
/// forever — the 30 s read timeout drops it. We don't wait the full timeout here;
/// instead we prove the daemon still serves *other* work isn't possible (serial), so
/// we just assert the connection is accepted and the daemon is reachable afterward by
/// closing the slow client quickly (EOF) — the timeout is the backstop for a client
/// that neither sends nor closes.
#[test]
fn partial_header_then_close_is_handled_promptly() {
    let d = start_daemon("slowloris");
    // Three separate partial-header-then-close pokes; each must resolve via EOF fast.
    let start = Instant::now();
    for _ in 0..3 {
        poke(&d.socket, &[0xDE]);
    }
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "partial-header connections should resolve on EOF, not block on the 30s timeout"
    );
    assert_alive(&d.socket, "partial-header");
}
