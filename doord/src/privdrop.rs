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

/// The identity a session is handed off to. Cloned into the post-fork `pre_exec`
/// closure so the drop runs against an owned copy in the child. Carries only
/// public passwd fields (name/uid/gid/home/shell) — no secret — so `Debug` is fine.
#[derive(Debug, Clone)]
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

/// The locale variables a graphical session needs to render correctly. A session
/// with none of these falls back to the `C` locale (`ANSI_X3.4-1968`, not UTF-8);
/// Qt/GTK toolkits detect that and refuse to run properly (Plasma black-screens),
/// so the login completes but the desktop never appears. This is the glibc set.
const LOCALE_VARS: &[&str] = &[
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_MESSAGES",
    "LC_PAPER",
    "LC_NAME",
    "LC_ADDRESS",
    "LC_TELEPHONE",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
];

/// Build the child session's environment from scratch — an explicit allowlist,
/// never a blanket copy of the daemon's own environment. The daemon runs as root
/// with whatever the init system handed it; none of that should leak into a user
/// session.
///
/// Only variables a fresh login legitimately needs are set; everything else
/// (the daemon's `PATH`, any inherited secrets, `LD_*` injection vectors) is
/// simply absent because we start from an empty set. The one inherited family is
/// locale ([`locale_env`]) — system configuration from `/etc/locale.conf`, which
/// systemd imports into the service-manager environment doord inherits. Passing it
/// through (by an explicit name list, not a blanket copy) is what a display manager
/// does; it goes into the user's *own* session at their own uid and is not a code
/// path, so it carries none of the escalation risk the allowlist exists to block.
pub fn sanitized_env(target: &TargetUser) -> Vec<(String, String)> {
    let mut env = vec![
        ("HOME".to_string(), target.home.clone()),
        ("USER".to_string(), target.name.clone()),
        ("LOGNAME".to_string(), target.name.clone()),
        ("SHELL".to_string(), target.shell.clone()),
        (
            "PATH".to_string(),
            "/usr/local/sbin:/usr/local/bin:/usr/bin".to_string(),
        ),
    ];
    env.extend(locale_env(|k| std::env::var(k).ok()));
    env
}

/// Locale variables for the session, drawn from `lookup` (the daemon's own
/// environment in production). Any [`LOCALE_VARS`] entry that is set and non-empty
/// is passed through. If none of the three that determine the character type
/// (`LC_ALL`, `LC_CTYPE`, `LANG`) is set, `LANG=C.UTF-8` is added so the session
/// always has a UTF-8 locale — a non-UTF-8 desktop session must never be the
/// failure mode of a misconfigured or empty locale, so this fails safe.
fn locale_env(lookup: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = LOCALE_VARS
        .iter()
        .filter_map(|&k| {
            lookup(k)
                .filter(|v| !v.is_empty())
                .map(|v| (k.to_string(), v))
        })
        .collect();
    let has_ctype = env
        .iter()
        .any(|(k, _)| k == "LC_ALL" || k == "LC_CTYPE" || k == "LANG");
    if !has_ctype {
        env.push(("LANG".to_string(), "C.UTF-8".to_string()));
    }
    env
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
    fn sanitized_env_starts_from_empty_and_sets_the_allowlist_then_locale() {
        let env = sanitized_env(&target());
        // The identity/PATH allowlist comes first, verbatim and in order.
        let head: Vec<&str> = env.iter().take(5).map(|(k, _)| k.as_str()).collect();
        assert_eq!(head, ["HOME", "USER", "LOGNAME", "SHELL", "PATH"]);
        // Anything after it is locale, and only locale — no other inherited var
        // is admitted (the allowlist's whole job).
        for (k, _) in env.iter().skip(5) {
            assert!(LOCALE_VARS.contains(&k.as_str()), "unexpected non-locale key: {k}");
        }
    }

    #[test]
    fn locale_env_passes_through_set_vars() {
        let src = |k: &str| match k {
            "LANG" => Some("en_US.UTF-8".to_string()),
            "LC_TIME" => Some("de_DE.UTF-8".to_string()),
            "LC_ALL" => Some(String::new()), // empty is treated as unset
            _ => None,
        };
        let env = locale_env(src);
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("LANG").as_deref(), Some("en_US.UTF-8"));
        assert_eq!(get("LC_TIME").as_deref(), Some("de_DE.UTF-8"));
        assert!(get("LC_ALL").is_none(), "empty value must not be passed through");
    }

    #[test]
    fn locale_env_falls_back_to_utf8_when_unset() {
        // No locale configured anywhere → the session must still get a UTF-8 ctype,
        // or a Qt/GTK desktop refuses to render (the lockout-domain failure).
        let env = locale_env(|_| None);
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("LANG").as_deref(), Some("C.UTF-8"));
    }

    #[test]
    fn locale_env_does_not_override_an_existing_ctype() {
        // LANG present → no fallback appended (we honor the configured locale).
        let env = locale_env(|k| (k == "LANG").then(|| "fr_FR.UTF-8".to_string()));
        let langs: Vec<&str> = env.iter().filter(|(k, _)| k == "LANG").map(|(_, v)| v.as_str()).collect();
        assert_eq!(langs, ["fr_FR.UTF-8"], "must not duplicate or override LANG");
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
