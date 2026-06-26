//! A throwaway interactive client for manually exercising a running `doord`
//! over its real socket: handshake, then drive a PAM conversation by typing.
//!
//! This is NOT the greeter and not part of the TCB — it is a test harness to
//! prove the live auth path end to end before the real greeter exists. It reads
//! replies from stdin (and, yes, echoes them — it is a localhost probe, not a
//! login screen).
//!
//! Run against a daemon you started with a matching DOORD_SOCKET, e.g.:
//!     DOORD_SOCKET=/run/doord-test.sock cargo run -p doord --example login_probe

use std::io::{self, BufRead, Write};
use std::os::unix::net::UnixStream;

use protocol::{read_frame, write_frame, AuthPrompt, Request, Response, PROTOCOL_VERSION, Secret};

fn main() {
    let socket = std::env::var("DOORD_SOCKET").unwrap_or_else(|_| "/run/doord/door.sock".to_string());
    let mut conn = UnixStream::connect(&socket)
        .unwrap_or_else(|e| panic!("connect to {socket}: {e} (is doord running, and can your uid reach the socket?)"));
    println!("connected to {socket}");

    // Handshake.
    write_frame(&mut conn, &Request::Hello { protocol_version: PROTOCOL_VERSION }).unwrap();
    match read_frame::<_, Response>(&mut conn).unwrap() {
        Response::Welcome { protocol_version } => println!("handshake ok (daemon speaks v{protocol_version})"),
        other => {
            eprintln!("handshake failed: {other:?}");
            return;
        }
    }

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    // List the startable sessions, as a real greeter would for its picker.
    write_frame(&mut conn, &Request::ListSessions).unwrap();
    match read_frame::<_, Response>(&mut conn).unwrap() {
        Response::Sessions(sessions) => {
            println!("sessions ({}):", sessions.len());
            for s in &sessions {
                match &s.comment {
                    Some(c) => println!("  {} — {} ({c})", s.id, s.name),
                    None => println!("  {} — {}", s.id, s.name),
                }
            }
        }
        other => println!("(could not list sessions: {other:?})"),
    }

    print!("username: ");
    io::stdout().flush().unwrap();
    let username = lines.next().and_then(|l| l.ok()).unwrap_or_default();

    write_frame(&mut conn, &Request::BeginAuth { username }).unwrap();

    // Pump the conversation until it terminates.
    loop {
        match read_frame::<_, Response>(&mut conn).unwrap() {
            Response::Auth(AuthPrompt::Question { text, secret }) => {
                print!("{text} {}", if secret { "(input echoes!) " } else { "" });
                io::stdout().flush().unwrap();
                let reply = lines.next().and_then(|l| l.ok()).unwrap_or_default();
                write_frame(&mut conn, &Request::AuthReply { response: Secret::new(reply) }).unwrap();
            }
            Response::Auth(AuthPrompt::Info { text }) => println!("[info] {text}"),
            Response::Auth(AuthPrompt::Error { text }) => println!("[error] {text}"),
            Response::AuthSuccess => {
                println!("✓ AUTH SUCCESS");
                break;
            }
            Response::AuthFailure { reason } => {
                println!("✗ AUTH FAILURE: {reason}");
                return;
            }
            other => {
                eprintln!("unexpected response: {other:?}");
                return;
            }
        }
    }

    // Authenticated — now ask the daemon to start a session. This is the
    // privilege handoff: the daemon forks, drops to the authenticated user, and
    // execs the session's Exec=. Point DOORD at a session whose Exec runs `id`
    // to watch the drop land (it reports the user's uid/gid, not root's).
    print!("session id to start: ");
    io::stdout().flush().unwrap();
    let session_id = lines.next().and_then(|l| l.ok()).unwrap_or_default();

    write_frame(&mut conn, &Request::Start { session_id }).unwrap();
    match read_frame::<_, Response>(&mut conn).unwrap() {
        Response::Started => println!("✓ SESSION STARTED (daemon now owns it; watch doord's output)"),
        Response::Error { message } => println!("✗ START REFUSED: {message}"),
        other => eprintln!("unexpected response: {other:?}"),
    }
}
