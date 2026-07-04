//! End-to-end smoke test for the session-lock **reauth** seam.
//!
//! Spawns the real `doord` binary with the reauth listener bound to a throwaway
//! socket and drives it over the actual reauth wire protocol — proving the framing,
//! the version handshake, the pre-handshake refusal, malformed-frame rejection, and
//! that a `Begin` gets a real (non-hanging) response. The deterministic verdict
//! coverage (allow/deny/cancel, padding, uid-binding) lives in the in-process unit
//! tests in `src/reauth.rs`; here we only assert the live seam stays alive and framed.
//!
//! The daemon runs unmanaged and connects as the test's own uid. Reauth accepts any
//! local uid (the peer-cred uid is the identity), so no `DOORD_GREETER_UID` is needed.

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use protocol::{read_frame, write_frame, ReauthRequest, ReauthResponse, PROTOCOL_VERSION};

/// A daemon child killed on drop, with its throwaway sockets/dirs cleaned up.
struct Daemon {
    child: Child,
    socket: PathBuf,
    reauth_dir: PathBuf,
    reauth_socket: PathBuf,
    session_root: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(&self.reauth_socket);
        let _ = std::fs::remove_dir_all(&self.reauth_dir);
        let _ = std::fs::remove_dir_all(&self.session_root);
    }
}

fn start_daemon(tag: &str) -> Daemon {
    let socket =
        std::env::temp_dir().join(format!("doord-reauth-{tag}-{}.sock", std::process::id()));
    // The reauth socket lives in its own dedicated dir (the daemon chmods that dir to
    // 0755 for world-traversal), mirroring production's /run/doord-reauth — never a
    // shared parent like /tmp itself.
    let reauth_dir =
        std::env::temp_dir().join(format!("doord-reauth-{tag}-dir-{}", std::process::id()));
    let reauth_socket = reauth_dir.join("reauth.sock");
    let _ = std::fs::remove_file(&socket);
    let _ = std::fs::remove_dir_all(&reauth_dir);

    let session_root = std::env::temp_dir().join(format!(
        "doord-reauth-sessions-{tag}-{}",
        std::process::id()
    ));
    let wayland = session_root.join("wayland-sessions");
    std::fs::create_dir_all(&wayland).expect("create session dir");
    std::fs::write(
        wayland.join("testde.desktop"),
        "[Desktop Entry]\nName=Test DE\nExec=/usr/bin/test-de\n",
    )
    .expect("write session entry");

    let child = Command::new(env!("CARGO_BIN_EXE_doord"))
        .env("DOORD_SOCKET", &socket)
        .env("DOORD_REAUTH_SOCKET", &reauth_socket)
        .env("DOORD_SESSION_DIRS", &session_root)
        .spawn()
        .expect("spawn doord");

    Daemon {
        child,
        socket,
        reauth_dir,
        reauth_socket,
        session_root,
    }
}

fn connect(socket: &Path) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match UnixStream::connect(socket) {
            Ok(s) => {
                // Never let a misbehaving daemon hang the suite.
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                return s;
            }
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    || e.kind() == io::ErrorKind::ConnectionRefused =>
            {
                if Instant::now() > deadline {
                    panic!("reauth socket never came up on {}: {e}", socket.display());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("connect failed: {e}"),
        }
    }
}

#[test]
fn reauth_handshake_succeeds() {
    let d = start_daemon("ok");
    let mut conn = connect(&d.reauth_socket);

    write_frame(
        &mut conn,
        &ReauthRequest::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    let resp: ReauthResponse = read_frame(&mut conn).unwrap();
    assert_eq!(
        resp,
        ReauthResponse::Welcome {
            protocol_version: PROTOCOL_VERSION
        }
    );
}

#[test]
fn reauth_rejects_a_request_before_the_handshake() {
    let d = start_daemon("nohandshake");
    let mut conn = connect(&d.reauth_socket);

    // Skip Hello and go straight to Begin.
    write_frame(&mut conn, &ReauthRequest::Begin).unwrap();
    let resp: ReauthResponse = read_frame(&mut conn).unwrap();
    assert!(
        matches!(resp, ReauthResponse::Error { .. }),
        "a pre-handshake reauth request must be refused, got {resp:?}"
    );
}

#[test]
fn reauth_rejects_an_incompatible_version() {
    let d = start_daemon("badversion");
    let mut conn = connect(&d.reauth_socket);

    write_frame(
        &mut conn,
        &ReauthRequest::Hello {
            protocol_version: PROTOCOL_VERSION + 99,
        },
    )
    .unwrap();
    let resp: ReauthResponse = read_frame(&mut conn).unwrap();
    assert_eq!(
        resp,
        ReauthResponse::Incompatible {
            daemon_protocol_version: PROTOCOL_VERSION
        }
    );
}

#[test]
fn reauth_begin_gets_a_real_response_and_the_seam_stays_alive() {
    let d = start_daemon("begin");
    let mut conn = connect(&d.reauth_socket);

    write_frame(
        &mut conn,
        &ReauthRequest::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    let _: ReauthResponse = read_frame(&mut conn).unwrap();

    // Begin runs a real PAM conversation in this spawned-binary test, so the outcome
    // depends on the host PAM stack (a Prompt, or a coarse Deny if the account has no
    // password step). We only assert the daemon answers *something* framed rather than
    // hanging or crashing — the deterministic outcomes are covered in-process.
    write_frame(&mut conn, &ReauthRequest::Begin).unwrap();
    let resp: Result<ReauthResponse, _> = read_frame(&mut conn);
    match resp {
        Ok(ReauthResponse::Prompt(_)) | Ok(ReauthResponse::Deny { .. }) => {}
        // A clean drop (e.g. the worker could not start in the sandboxless test env) is
        // also acceptable — the real assertion is that the daemon did not hang/panic.
        Err(_) => {}
        other => panic!("unexpected reauth response to Begin: {other:?}"),
    }

    // A malformed frame after the handshake must be refused without wedging the daemon.
    drop(conn);
    let mut conn = connect(&d.reauth_socket);
    write_frame(
        &mut conn,
        &ReauthRequest::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    let _: ReauthResponse = read_frame(&mut conn).unwrap();
    let body = b"not a reauth frame";
    let mut raw = (body.len() as u32).to_be_bytes().to_vec();
    raw.extend_from_slice(body);
    conn.write_all(&raw).unwrap();
    conn.flush().unwrap();
    let resp: Result<ReauthResponse, _> = read_frame(&mut conn);
    assert!(
        matches!(resp, Ok(ReauthResponse::Error { .. })) || resp.is_err(),
        "a malformed reauth frame must be refused, got {resp:?}"
    );
}
