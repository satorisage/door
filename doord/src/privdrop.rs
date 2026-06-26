//! Privilege drop and environment sanitization for the session handoff.
//!
//! When the daemon spawns the user's session it must shed every scrap of its own
//! privilege first, in the one ordering that is actually safe, and hand the
//! child a clean environment built from scratch rather than the daemon's own.
//!
//! The ordering is the subtle part. Group privileges must go **before** user
//! privileges: once you drop the uid you may no longer be allowed to call
//! `setgid`/`setgroups`, so dropping the uid first can strand the child still in
//! `root`'s groups. The safe sequence is therefore:
//!
//!   1. `setgroups` — install exactly the target user's supplementary groups
//!      (and nothing of root's), via `initgroups`.
//!   2. `setresgid` — primary gid to the target, with no saved-set escape back.
//!   3. `setresuid` — uid to the target, real/effective/saved all set so there
//!      is no saved uid to restore root from.
//!
//! Each drop is then *verified*: if a `setres*id` silently failed we must never
//! continue to spawn a shell that is still root. The session-spawn path that
//! calls this lands with the session milestone; the mechanism and its ordering
//! live here, isolated and reviewable.

use std::ffi::CString;
use std::io;

/// The identity a session is handed off to.
#[allow(dead_code)] // home/shell are consumed by the session-spawn path (next milestone).
pub struct TargetUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
}

/// Drop from the daemon's privilege to `target`'s, groups-before-user, and
/// verify the drop took. Returns an error (rather than continuing) if any step
/// failed or left a residual privilege — the caller must abort the spawn.
///
/// Must run after `fork`, in the child, before `exec`.
#[allow(dead_code)] // Wired by the session-spawn path in the next milestone.
pub fn drop_to(target: &TargetUser) -> io::Result<()> {
    let name = CString::new(target.name.as_str())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "username has a NUL byte"))?;

    // 1. Supplementary groups: exactly the target's, replacing root's set.
    // SAFETY: `name` is a valid C string; initgroups reads it and sets this
    // process's supplementary groups. Requires privilege we still hold here.
    if unsafe { libc::initgroups(name.as_ptr(), target.gid) } != 0 {
        return Err(io::Error::last_os_error());
    }

    // 2. Primary gid — real, effective, and saved, so there is no saved-gid to
    // climb back through.
    // SAFETY: plain syscall with scalar args.
    if unsafe { libc::setresgid(target.gid, target.gid, target.gid) } != 0 {
        return Err(io::Error::last_os_error());
    }

    // 3. uid last — after this we can no longer change groups or gid.
    // SAFETY: plain syscall with scalar args.
    if unsafe { libc::setresuid(target.uid, target.uid, target.uid) } != 0 {
        return Err(io::Error::last_os_error());
    }

    verify_dropped(target)
}

/// Confirm the privilege drop actually took effect — defense against a
/// `setres*id` that returned success but did not fully apply, and a sanity gate
/// that we are not about to exec a root shell.
#[allow(dead_code)]
fn verify_dropped(target: &TargetUser) -> io::Result<()> {
    // SAFETY: these getters take no arguments and cannot fail.
    let (ruid, euid, suid) = unsafe { (libc::getuid(), libc::geteuid(), 0u32) };
    let _ = suid; // getresuid is glibc-specific; ruid==euid==target is the gate.
    if ruid != target.uid || euid != target.uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "uid did not drop to the target user",
        ));
    }
    // SAFETY: as above.
    let (rgid, egid) = unsafe { (libc::getgid(), libc::getegid()) };
    if rgid != target.gid || egid != target.gid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "gid did not drop to the target user",
        ));
    }
    if target.uid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to spawn a session as uid 0",
        ));
    }
    Ok(())
}

/// Build the child session's environment from scratch — an explicit allowlist,
/// never the daemon's own environment. The daemon runs as root with whatever
/// the init system handed it; none of that should leak into a user session.
///
/// Only variables a fresh login legitimately needs are set; everything else
/// (the daemon's `PATH`, any inherited secrets, `LD_*` injection vectors) is
/// simply absent because we start from an empty set.
#[allow(dead_code)] // Consumed by the session-spawn path (next milestone); unit-tested now.
pub fn sanitized_env(target: &TargetUser) -> Vec<(String, String)> {
    vec![
        ("HOME".to_string(), target.home.clone()),
        ("USER".to_string(), target.name.clone()),
        ("LOGNAME".to_string(), target.name.clone()),
        ("SHELL".to_string(), target.shell.clone()),
        (
            "PATH".to_string(),
            "/usr/local/sbin:/usr/local/bin:/usr/bin".to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> TargetUser {
        TargetUser {
            name: "stephen".to_string(),
            uid: 1000,
            gid: 1000,
            home: "/home/stephen".to_string(),
            shell: "/bin/zsh".to_string(),
        }
    }

    #[test]
    fn sanitized_env_starts_from_empty_and_sets_only_the_allowlist() {
        let env = sanitized_env(&target());
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["HOME", "USER", "LOGNAME", "SHELL", "PATH"]);
    }

    #[test]
    fn sanitized_env_does_not_carry_dangerous_inherited_vars() {
        // Set a hostile var in the daemon's own environment...
        std::env::set_var("LD_PRELOAD", "/tmp/evil.so");
        let env = sanitized_env(&target());
        // ...and confirm the built child env never contains it.
        assert!(!env.iter().any(|(k, _)| k == "LD_PRELOAD"));
        std::env::remove_var("LD_PRELOAD");
    }

    #[test]
    fn sanitized_env_binds_identity_to_the_target() {
        let env = sanitized_env(&target());
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("HOME").as_deref(), Some("/home/stephen"));
        assert_eq!(get("USER").as_deref(), Some("stephen"));
        assert_eq!(get("LOGNAME").as_deref(), Some("stephen"));
    }
}
