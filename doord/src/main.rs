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
mod fdpass;
mod hardening;
mod ipc;
mod pam;
mod privdrop;
mod sessions;
mod spawn;
// M5 sandbox groundwork: the pre-forked spawner + its supervisor-side helpers. It
// creates both the session worker and the greeter worker outside the supervisor's
// lineage so a future supervisor self-sandbox never confines the desktop or the
// greeter compositor. Enabled by default via DOORD_SPAWNER (the shipped unit sets
// it); unset it to fall back to the direct in-lineage path.
mod spawner;
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

    // Child creation: with DOORD_SPAWNER (the shipped default), a spawner is forked
    // here — after the baseline but before serving — that owns creation of both the
    // greeter worker and the per-login session worker, so a future supervisor
    // self-sandbox is never inherited by the desktop or the greeter compositor.
    // Fails open to the direct in-lineage path if the spawner can't be forked; unset
    // the flag to select that path deliberately.
    let spawner = if std::env::var_os("DOORD_SPAWNER").is_some() {
        match spawner::fork_spawner(pam::spawn_raw) {
            Ok(sock) => {
                eprintln!(
                    "doord: spawner mode — the greeter and session workers are created by a \
                     pre-forked helper"
                );
                Some(spawner::Spawner::new(sock))
            }
            Err(e) => {
                eprintln!("doord: could not fork the spawner ({e}); using the direct in-lineage path");
                None
            }
        }
    } else {
        None
    };
    // Confine the supervisor's syscall surface (seccomp), now that the spawner owns
    // session/greeter creation off this lineage. Applied here — after fork_spawner,
    // before serving — and only in spawner mode: on the direct in-lineage path the
    // supervisor itself forks the session, so a seccomp filter, being inherited
    // across fork and preserved across execve, would confine the desktop and break
    // every login.
    // Best-effort: a failed install (or `off`) leaves the supervisor running with
    // the baseline only, and says so. `DOORD_SECCOMP=log` records unlisted syscalls
    // without blocking; `enforce` fails them with EPERM; `DOORD_NO_SANDBOX=1` forces off.
    if spawner.is_some() {
        let mode = hardening::SeccompMode::from_env();
        match hardening::apply_seccomp(mode) {
            Ok(()) if mode != hardening::SeccompMode::Off => {
                eprintln!("doord: supervisor seccomp filter installed ({mode:?})");
            }
            Ok(()) => {}
            Err(e) => {
                eprintln!(
                    "doord: warning: could not apply the seccomp filter ({mode:?}); \
                     continuing with the baseline only: {e}"
                );
            }
        }
    } else if hardening::SeccompMode::from_env() != hardening::SeccompMode::Off {
        eprintln!(
            "doord: DOORD_SECCOMP set but not in spawner mode; skipping — a supervisor \
             filter on the direct in-lineage path would confine the desktop session"
        );
    }

    // Confine the supervisor's filesystem reach (Landlock), after seccomp and under the
    // same rule: spawner-only, so the desktop/greeter — forked off the spawner, not this
    // lineage — never inherits the path ruleset. Landlock has no permissive mode, so this
    // ships flag-gated + default-off (`DOORD_LANDLOCK=enforce` opts a genny boot in; the
    // path seed is tuned there before the shipped unit flips). Best-effort: an old kernel
    // degrades to NotEnforced. `DOORD_NO_SANDBOX=1` forces off.
    if spawner.is_some() {
        let mode = hardening::LandlockMode::from_env();
        match hardening::apply_landlock(mode) {
            Ok(status) if mode != hardening::LandlockMode::Off => {
                eprintln!("doord: supervisor Landlock ruleset installed ({mode:?}, {status:?})");
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "doord: warning: could not apply the Landlock ruleset ({mode:?}); \
                     continuing without a path sandbox: {e}"
                );
            }
        }
    } else if hardening::LandlockMode::from_env() != hardening::LandlockMode::Off {
        eprintln!(
            "doord: DOORD_LANDLOCK set but not in spawner mode; skipping — a supervisor \
             path ruleset on the direct in-lineage path would confine the desktop session"
        );
    }

    let logins = WorkerLoginFactory::new();

    match ipc::serve(&config, &logins, spawner.as_ref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!(
                "doord: fatal: could not serve on {}: {e}",
                config.socket_path.display()
            );
            ExitCode::FAILURE
        }
    }
}
