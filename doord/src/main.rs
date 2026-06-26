//! doord — the privileged door daemon.
//!
//! This is the trusted computing base. It is the only part of door that runs
//! with privilege, and it owns every privileged responsibility end to end:
//!
//!   - the PAM authentication conversation,
//!   - seat / VT ownership and session bookkeeping via logind,
//!   - discovering startable sessions and spawning the chosen one with a
//!     sanitized environment,
//!   - the local IPC socket that the unprivileged greeter connects to.
//!
//! Everything arriving from the greeter is untrusted: the daemon validates it,
//! acts only on requests it permits, and drops privilege as early as it can.
//! Keeping this binary small is a security property, not a style preference —
//! every line here is attack surface that runs as root.
//!
//! Built so far: process hardening baseline; the authorized, framed IPC seam;
//! the PAM auth conversation; session discovery; and the full session handoff —
//! one PAM transaction spanning auth → logind session open (`pam_systemd`) →
//! fork → seat/VT controlling-tty handoff → privilege drop → exec, with the
//! logind session closed when it exits. The end-to-end live logind run on real
//! hardware is the remaining verification.

mod config;
mod hardening;
mod ipc;
mod pam;
mod privdrop;
mod sessions;
mod spawn;
mod user;
mod worker;

use std::process::ExitCode;

use config::Config;
use pam::WorkerLoginFactory;

fn main() -> ExitCode {
    // Re-exec modes (the daemon forks itself into these): the per-login session
    // worker (D-0005) holds the PAM transaction and is the logind session leader;
    // the greeter worker (D-0008) holds a passwordless greeter session and runs
    // cage. Neither opens the IPC listener.
    match std::env::args().nth(1).as_deref() {
        Some(worker::WORKER_ARG) => return worker::main(),
        Some(worker::GREETER_WORKER_ARG) => return worker::run_greeter(),
        _ => {}
    }

    // Lock down the process before opening any attack surface.
    hardening::apply_baseline();

    let config = Config::from_env();
    let logins = WorkerLoginFactory::new();

    match ipc::serve(&config, &logins) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("doord: fatal: could not serve on {}: {e}", config.socket_path.display());
            ExitCode::FAILURE
        }
    }
}
