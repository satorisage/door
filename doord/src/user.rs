//! Resolving an authenticated username to the system identity a session runs as.
//!
//! After PAM says "yes, this is `stephen`", the spawn path still needs the
//! concrete uid/gid/home/shell to drop to — facts that live in the passwd
//! database, not in anything the greeter sent. This module is the single place
//! that consults `getpwnam` and yields a [`TargetUser`] the privilege drop and
//! environment build consume.
//!
//! The username it resolves is always the one PAM authenticated, never one the
//! greeter supplied for the spawn: the greeter's `Start` carries only a session
//! id, so the identity here cannot be a greeter-chosen one. That binding is the
//! point — a compromised greeter must not be able to start a session *as someone
//! else*.

use std::ffi::CStr;
use std::io;

use crate::privdrop::TargetUser;

/// Look up `username` in the passwd database and return the identity to hand a
/// session off to. Errors if the user does not exist or the lookup fails — the
/// caller must then refuse to spawn rather than fall back to any default.
pub fn resolve(username: &str) -> io::Result<TargetUser> {
    let c_name = std::ffi::CString::new(username)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "username has a NUL byte"))?;

    // getpwnam_r writes into a caller-provided buffer; the right size is not
    // known up front, so grow on ERANGE. The suggested starting size comes from
    // sysconf, with a sane floor for systems that report no hint.
    let mut buf_len = match unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) } {
        n if n > 0 => n as usize,
        _ => 1024,
    };

    loop {
        let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let mut buf = vec![0i8; buf_len];

        // SAFETY: all pointers reference live, correctly-sized storage held by
        // this stack frame for the duration of the call; `c_name` is a valid C
        // string. getpwnam_r writes the entry into `passwd`/`buf` and sets
        // `result` to `&passwd` on success or null if no such user.
        let ret = unsafe {
            libc::getpwnam_r(
                c_name.as_ptr(),
                &mut passwd,
                buf.as_mut_ptr(),
                buf_len,
                &mut result,
            )
        };

        if ret == libc::ERANGE {
            // Buffer too small for this entry; double it and retry.
            buf_len = buf_len.saturating_mul(2);
            continue;
        }
        if ret != 0 {
            return Err(io::Error::from_raw_os_error(ret));
        }
        if result.is_null() {
            // ret == 0 with a null result means: no such user.
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no such user '{username}'"),
            ));
        }

        // SAFETY: these passwd fields point into `buf`, which outlives the reads
        // here; getpwnam_r guarantees them NUL-terminated.
        let name = unsafe { cstr_to_string(passwd.pw_name) };
        let home = unsafe { cstr_to_string(passwd.pw_dir) };
        let mut shell = unsafe { cstr_to_string(passwd.pw_shell) };
        // A passwd entry with an empty shell field conventionally means /bin/sh.
        if shell.is_empty() {
            shell = "/bin/sh".to_string();
        }

        return Ok(TargetUser {
            name,
            uid: passwd.pw_uid,
            gid: passwd.pw_gid,
            home,
            shell,
        });
    }
}

/// Copy a C string field out of a passwd entry into an owned `String`.
///
/// SAFETY: `ptr` must be a valid, NUL-terminated C string (or null). A null or
/// non-UTF-8 field yields an empty / lossy string rather than failing — a login
/// must not be blocked by an oddly-encoded gecos-adjacent field.
unsafe fn cstr_to_string(ptr: *const libc::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_root_to_uid_zero() {
        // `root` exists on every Unix; its uid/gid are fixed at 0. (We only
        // resolve here — refusing to *drop* to uid 0 is privdrop's job.)
        let root = resolve("root").expect("root must resolve");
        assert_eq!(root.name, "root");
        assert_eq!(root.uid, 0);
        assert_eq!(root.gid, 0);
        assert!(!root.shell.is_empty());
    }

    #[test]
    fn unknown_user_is_not_found() {
        let err = resolve("doord-no-such-user-7f3a9c").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn username_with_nul_is_rejected() {
        let err = resolve("ro\0ot").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
