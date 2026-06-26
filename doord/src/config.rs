//! Where the daemon's runtime knobs come from.
//!
//! In production these are fixed by the install: a socket under `/run/doord` and
//! the greeter system user's uid/gid. They are overridable by environment for
//! development and testing, so the seam can be exercised without being root or
//! installing a system user — the *mechanism* (peercred, perms, framing) is
//! identical either way; only the identities differ.

use std::ffi::OsString;
use std::path::PathBuf;

/// Default production socket path. The parent dir is created `root:root 0700`.
const DEFAULT_SOCKET_PATH: &str = "/run/doord/door.sock";

/// Default PAM service name. Resolves to `/etc/pam.d/doord` (installed by door),
/// falling back to `/etc/pam.d/other` — which denies — if door's file is absent.
const DEFAULT_PAM_SERVICE: &str = "doord";

/// Resolved daemon configuration.
pub struct Config {
    /// Pathname Unix socket to listen on (`DOORD_SOCKET`).
    pub socket_path: PathBuf,
    /// The only uid permitted to connect, enforced via `SO_PEERCRED`
    /// (`DOORD_GREETER_UID`; defaults to the daemon's own uid for dev runs).
    pub greeter_uid: u32,
    /// Group to own the socket, if known (`DOORD_GREETER_GID`); `None` skips the
    /// chgrp, leaving the peercred check as the sole authorization gate.
    pub greeter_gid: Option<u32>,
    /// PAM service name to authenticate against (`DOORD_PAM_SERVICE`).
    pub pam_service: String,
}

impl Config {
    /// Build the configuration from the environment, falling back to
    /// development-friendly defaults so an unprivileged `cargo run` still serves.
    pub fn from_env() -> Self {
        let socket_path = env_os("DOORD_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));

        // Default the allowed peer to whoever launched the daemon, so a dev run
        // authorizes its own test greeter without configuration.
        let greeter_uid = env_u32("DOORD_GREETER_UID").unwrap_or_else(current_uid);

        let greeter_gid = env_u32("DOORD_GREETER_GID");

        let pam_service = std::env::var("DOORD_PAM_SERVICE")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_PAM_SERVICE.to_string());

        Config {
            socket_path,
            greeter_uid,
            greeter_gid,
            pam_service,
        }
    }
}

fn env_os(key: &str) -> Option<OsString> {
    std::env::var_os(key).filter(|v| !v.is_empty())
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|v| v.trim().parse().ok())
}

fn current_uid() -> u32 {
    // SAFETY: getuid is always safe; it takes no arguments and cannot fail.
    unsafe { libc::getuid() }
}
