//! End-to-end smoke test for the IPC seam.
//!
//! Spawns the real `doord` binary on a throwaway socket and drives it over the
//! actual wire protocol — proving the framing, the version handshake, the
//! peercred authorization (the test connects as its own uid, which the daemon
//! defaults to allowing), and the stubbed dispatch all line up. This is the
//! "demonstrable, not reported" check for the M1 IPC-server task.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use protocol::{read_frame, write_frame, Request, Response, Session, PROTOCOL_VERSION};

/// A daemon child that is killed when the handle drops, so a failed assertion
/// never leaks a running process. Owns a throwaway session data-dir so discovery
/// is deterministic and isolated from whatever sessions the host has installed.
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
    let socket = std::env::temp_dir().join(format!("doord-test-{tag}-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);

    // A controlled session root: exactly one known Wayland session, so the
    // discovery answer is deterministic regardless of the host's installed DEs.
    let session_root =
        std::env::temp_dir().join(format!("doord-test-sessions-{tag}-{}", std::process::id()));
    let wayland = session_root.join("wayland-sessions");
    std::fs::create_dir_all(&wayland).expect("create session dir");
    std::fs::write(
        wayland.join("testde.desktop"),
        "[Desktop Entry]\nName=Test DE\nComment=for the smoke test\nExec=/usr/bin/test-de\n",
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

fn connect(socket: &PathBuf) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match UnixStream::connect(socket) {
            Ok(s) => return s,
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

#[test]
fn handshake_then_serves_requests() {
    let daemon = start_daemon("ok");
    let mut conn = connect(&daemon.socket);

    // Handshake: Hello → Welcome.
    write_frame(
        &mut conn,
        &Request::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    let resp: Response = read_frame(&mut conn).unwrap();
    assert_eq!(
        resp,
        Response::Welcome {
            protocol_version: PROTOCOL_VERSION
        }
    );

    // ListSessions discovers the controlled session root and returns the wire
    // projection (id/name/comment) — the daemon-side `Exec` never crosses.
    write_frame(&mut conn, &Request::ListSessions).unwrap();
    let resp: Response = read_frame(&mut conn).unwrap();
    assert_eq!(
        resp,
        Response::Sessions(vec![Session {
            id: "testde".to_string(),
            name: "Test DE".to_string(),
            comment: Some("for the smoke test".to_string()),
        }])
    );

    // BeginAuth drives a real PAM conversation in this (spawned-binary) test, so
    // its outcome depends on the host PAM stack; the deterministic auth-flow
    // coverage lives in the in-process tests in src/ipc.rs. Here we only assert
    // the seam stays alive and serves the framed request/response protocol.
}

#[test]
fn rejects_a_request_before_the_handshake() {
    let daemon = start_daemon("nohandshake");
    let mut conn = connect(&daemon.socket);

    // Skip Hello and go straight for a privileged request.
    write_frame(
        &mut conn,
        &Request::BeginAuth {
            username: "stephen".to_string(),
        },
    )
    .unwrap();

    let resp: Response = read_frame(&mut conn).unwrap();
    assert!(
        matches!(resp, Response::Error { .. }),
        "a pre-handshake request must be refused, got {resp:?}"
    );
}

#[test]
fn rejects_an_incompatible_version() {
    let daemon = start_daemon("badversion");
    let mut conn = connect(&daemon.socket);

    write_frame(
        &mut conn,
        &Request::Hello {
            protocol_version: PROTOCOL_VERSION + 99,
        },
    )
    .unwrap();

    let resp: Response = read_frame(&mut conn).unwrap();
    assert_eq!(
        resp,
        Response::Incompatible {
            daemon_protocol_version: PROTOCOL_VERSION
        }
    );
}
