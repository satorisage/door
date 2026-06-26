//! Process-level hardening applied before the daemon does anything else.
//!
//! These are cheap, irreversible kernel toggles that shrink what an attacker who
//! later compromises the daemon can do. They are applied at startup, before the
//! IPC socket exists, so the credential-handling process runs hardened for its
//! whole life. The heavier sandbox (seccomp syscall allowlist, Landlock
//! filesystem bounding) lands in the dedicated hardening milestone; these two
//! are the baseline that costs nothing to set now.

use std::io;

/// Apply the always-on baseline:
///
/// - **`NO_NEW_PRIVS`** — no `execve` from this process (or any child) can ever
///   gain privileges via setuid/setgid/file capabilities. A spawned session can
///   only drop privilege, never escalate.
/// - **non-dumpable** — disables core dumps and blocks `ptrace`-attach by other
///   processes of the same user, so a credential sitting in this daemon's memory
///   cannot be scraped out of a core file or a debugger.
///
/// Best-effort: a failure is logged to the journal (stderr) but does not abort
/// startup, so the daemon still comes up on a kernel that refuses one toggle —
/// it just comes up less hardened, and says so.
pub fn apply_baseline() {
    if let Err(e) = set_no_new_privs() {
        eprintln!("doord: warning: could not set NO_NEW_PRIVS: {e}");
    }
    if let Err(e) = set_non_dumpable() {
        eprintln!("doord: warning: could not clear the dumpable flag: {e}");
    }
}

fn set_no_new_privs() -> io::Result<()> {
    // prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)
    let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_non_dumpable() -> io::Result<()> {
    // prctl(PR_SET_DUMPABLE, 0)
    let ret = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
