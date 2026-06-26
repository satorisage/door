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
//! Built so far: process hardening baseline + the authorized, framed IPC seam.
//! PAM, privilege drop, session discovery and spawn land on top of this socket
//! next.

mod config;
mod hardening;
mod ipc;
mod pam;
mod privdrop;
mod sessions;

use std::process::ExitCode;

use config::Config;
use pam::PamAuthenticator;

fn main() -> ExitCode {
    // Lock down the process before opening any attack surface.
    hardening::apply_baseline();

    let config = Config::from_env();
    let authenticator = PamAuthenticator::new(config.pam_service.clone());

    match ipc::serve(&config, &authenticator) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("doord: fatal: could not serve on {}: {e}", config.socket_path.display());
            ExitCode::FAILURE
        }
    }
}
