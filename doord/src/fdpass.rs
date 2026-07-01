//! Passing an open file descriptor between processes over a `UnixStream`, via an
//! `SCM_RIGHTS` ancillary message.
//!
//! The sandbox-enabling spawner split needs this: the pre-forked **spawner** creates
//! the per-login worker and its control socketpair, but the **supervisor** is the
//! process that proxies the PAM conversation over that control socket. So the
//! spawner must hand the supervisor an open fd — the supervisor's end of the
//! control socketpair — across their own channel. `SCM_RIGHTS` is the only way to
//! move a live fd between unrelated-at-the-time processes; the kernel installs a
//! *new* fd in the receiver referring to the same open file description.
//!
//! One fd per call, which is all the protocol needs. A one-byte data payload rides
//! with the control message because some kernels drop an ancillary-only `sendmsg`.

use std::io;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;

/// Ancillary buffer big enough for exactly one fd. `CMSG_SPACE(sizeof(RawFd))` is
/// 24 bytes on 64-bit Linux (a 16-byte `cmsghdr` + an 8-byte-aligned 4-byte fd); 32
/// is a safe over-allocation and keeps the buffer a round size.
const CMSG_BUF_LEN: usize = 32;

/// Send `fd` to the peer on `sock`. The fd stays open in this process; the peer
/// receives a new fd for the same open file description. Best-effort byte payload
/// of one `\0`.
pub fn send_fd(sock: &UnixStream, fd: RawFd) -> io::Result<()> {
    let mut payload = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr() as *mut libc::c_void,
        iov_len: payload.len(),
    };
    let mut cmsg_buf = [0u8; CMSG_BUF_LEN];

    // SAFETY: msghdr zeroed then fully initialized; the cmsg buffer is sized for one
    // fd; all pointers reference locals that outlive the `sendmsg` call.
    let sent = unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize;

        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null() {
            return Err(io::Error::other("CMSG_FIRSTHDR returned null"));
        }
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as usize;
        std::ptr::copy_nonoverlapping(
            &fd as *const RawFd as *const u8,
            libc::CMSG_DATA(cmsg),
            std::mem::size_of::<RawFd>(),
        );

        libc::sendmsg(sock.as_raw_fd(), &msg, 0)
    };

    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Receive one fd from the peer on `sock`. Returns an owned fd (closed on drop
/// unless the caller keeps it). Errors if the peer sent no `SCM_RIGHTS` control
/// message (e.g. it closed the socket).
pub fn recv_fd(sock: &UnixStream) -> io::Result<OwnedFd> {
    let mut payload = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr() as *mut libc::c_void,
        iov_len: payload.len(),
    };
    let mut cmsg_buf = [0u8; CMSG_BUF_LEN];

    // SAFETY: msghdr zeroed then initialized; the cmsg buffer receives at most one
    // fd's worth of ancillary data; pointers reference locals that outlive the call.
    let fd = unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = cmsg_buf.len();

        let n = libc::recvmsg(sock.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "peer closed the fd-passing socket",
            ));
        }
        // Reject a truncated control message rather than trust a partial fd.
        if msg.msg_flags & libc::MSG_CTRUNC != 0 {
            return Err(io::Error::other("received a truncated SCM_RIGHTS message"));
        }

        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null()
            || (*cmsg).cmsg_level != libc::SOL_SOCKET
            || (*cmsg).cmsg_type != libc::SCM_RIGHTS
        {
            return Err(io::Error::other("no SCM_RIGHTS fd in the message"));
        }
        let mut received: RawFd = -1;
        std::ptr::copy_nonoverlapping(
            libc::CMSG_DATA(cmsg),
            &mut received as *mut RawFd as *mut u8,
            std::mem::size_of::<RawFd>(),
        );
        received
    };

    if fd < 0 {
        return Err(io::Error::other("invalid fd received"));
    }
    // SAFETY: `fd` is a freshly installed, owned fd from the kernel (MSG_CMSG_CLOEXEC
    // set it close-on-exec); wrapping it in OwnedFd gives it a single owner.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::{FromRawFd, IntoRawFd};

    /// Round-trip a live pipe write-end across a socketpair: the received fd must
    /// refer to the *same* open pipe (a byte written through it appears on the read
    /// end), proving we moved a working fd, not just a number.
    #[test]
    fn passes_a_working_fd_across_a_socketpair() {
        let (a, b) = UnixStream::pair().unwrap();

        // A pipe whose write-end we will pass over the socket.
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read_end = fds[0];
        let write_end = fds[1];

        send_fd(&a, write_end).unwrap();
        // Drop our own copy so only the passed fd remains a writer.
        unsafe { libc::close(write_end) };

        let got = recv_fd(&b).unwrap();
        // Take sole ownership of the received fd into a File and write through it.
        let mut sender = unsafe { File::from_raw_fd(got.into_raw_fd()) };
        sender.write_all(b"X").unwrap();
        drop(sender); // close the write end

        let mut reader = unsafe { File::from_raw_fd(read_end) };
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"X");
    }

    #[test]
    fn recv_errors_when_peer_sends_no_fd() {
        let (a, b) = UnixStream::pair().unwrap();
        // Plain byte, no ancillary fd.
        (&a).write_all(b"z").unwrap();
        assert!(recv_fd(&b).is_err());
    }
}
