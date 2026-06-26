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

use protocol::{read_frame, write_frame, AuthPrompt, Request, Response, Secret, PROTOCOL_VERSION};

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
                return;
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
}
