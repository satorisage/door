//! The pre-forked spawner (M5 sandbox groundwork) — foundation.
//!
//! seccomp/Landlock are inherited across `fork` and preserved across `execve`, so a
//! sandbox the supervisor applies to itself would also land on the per-login worker
//! and the session it `execve`s (which the per-login worker spawns). To let the
//! supervisor confine itself
//! without confining the desktop, the worker must be created **outside** the
//! supervisor's sandboxed lineage.
//!
//! This spawner is forked once at startup, *before* any sandbox. It owns worker
//! creation: on the supervisor's request it forks/re-execs the worker, hands the
//! worker's control-socket fd back to the supervisor with `SCM_RIGHTS`
//! ([`crate::fdpass`]), then reaps the worker and reports its exit. Because the
//! worker is the spawner's child (not the sandboxed supervisor's), neither PAM nor
//! the session inherits the supervisor's sandbox.
//!
//! Trust surface: the spawner parses no untrusted input — it speaks only this tiny
//! codec to the supervisor over a socketpair, holds no greeter fd, and its only
//! power is "fork a worker on request." *When* that is allowed is still gated by
//! the supervisor (only after a peercred-authorized greeter completes an auth).
//!
//! Concurrency: logins are serial (one greeter, one seat), so at most one worker
//! exists at a time; the spawner handles a request, then blocks reaping that worker
//! until it exits before taking the next request.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;

use crate::fdpass;

/// Supervisor → spawner: "fork a worker."
const REQ_SPAWN: u8 = b'S';
/// Spawner → supervisor: worker started; a 4-byte LE pid follows, then an
/// `SCM_RIGHTS` control fd.
const EV_STARTED: u8 = b'R';
/// Spawner → supervisor: the spawn failed; no fd follows.
const EV_ERROR: u8 = b'E';
/// Spawner → supervisor: the worker exited; a 4-byte LE pid + 4-byte LE status follow.
const EV_EXITED: u8 = b'X';

/// How the spawner creates a worker: returns `(child_pid, supervisor_end_of_control)`.
/// Production will inject a `pam`-side worker fork; tests inject a stand-in.
pub type SpawnFn = fn() -> io::Result<(libc::pid_t, UnixStream)>;

/// Fork the spawner process (call at startup, before any sandbox). Returns the
/// supervisor's end of the control socket; the spawner runs [`run_spawner`] in the
/// child and never returns to the caller there. The spawner is armed with
/// `PR_SET_PDEATHSIG(SIGKILL)` so it dies with the supervisor.
pub fn fork_spawner(spawn: SpawnFn) -> io::Result<UnixStream> {
    let (supervisor_end, spawner_end) = UnixStream::pair()?;
    // SAFETY: fork in a process that is single-threaded at startup here. The child
    // path does only async-signal-safe work before entering its own serve loop.
    match unsafe { libc::fork() } {
        -1 => Err(io::Error::last_os_error()),
        0 => {
            drop(supervisor_end);
            // Die if the supervisor dies, so a supervisor crash never orphans the
            // spawner (which could otherwise keep spawning workers).
            unsafe {
                libc::prctl(
                    libc::PR_SET_PDEATHSIG,
                    libc::SIGKILL as libc::c_ulong,
                    0,
                    0,
                    0,
                )
            };
            run_spawner(spawner_end, spawn);
            // run_spawner only returns on a clean supervisor disconnect.
            unsafe { libc::_exit(0) };
        }
        _ => {
            drop(spawner_end);
            Ok(supervisor_end)
        }
    }
}

/// The spawner's serve loop. Reads requests, forks workers via `spawn`, passes the
/// control fd back, reaps each worker, and reports its exit. Returns on EOF (the
/// supervisor is gone).
pub fn run_spawner(mut sock: UnixStream, spawn: SpawnFn) {
    loop {
        let mut tag = [0u8; 1];
        match sock.read_exact(&mut tag) {
            Ok(()) => {}
            // Supervisor closed the channel — nothing more to do.
            Err(_) => return,
        }
        if tag[0] != REQ_SPAWN {
            // Unknown request; ignore rather than die (defensive).
            continue;
        }

        let (pid, control) = match spawn() {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("doord: spawner: could not create a worker: {e}");
                let _ = sock.write_all(&[EV_ERROR]);
                continue;
            }
        };

        // Report the worker + hand its control fd to the supervisor.
        let mut header = [0u8; 5];
        header[0] = EV_STARTED;
        header[1..].copy_from_slice(&pid.to_le_bytes());
        if sock.write_all(&header).is_err() || fdpass::send_fd(&sock, control.as_raw()).is_err() {
            // Supervisor vanished mid-reply; the worker will die on its own control
            // EOF and be reaped below (or on the next iteration).
            let _ = wait_pid(pid);
            return;
        }
        drop(control); // the supervisor now owns the control fd; the spawner does not

        // Block until the worker exits (serial model — no other request comes until
        // this login ends), then report its exit so the supervisor's session-wait
        // can complete.
        let status = wait_pid(pid);
        let mut ev = [0u8; 9];
        ev[0] = EV_EXITED;
        ev[1..5].copy_from_slice(&pid.to_le_bytes());
        ev[5..9].copy_from_slice(&status.to_le_bytes());
        if sock.write_all(&ev).is_err() {
            return;
        }
    }
}

/// `waitpid` a worker to completion, returning its raw wait status (or -1 on error).
fn wait_pid(pid: libc::pid_t) -> i32 {
    let mut status: libc::c_int = 0;
    // SAFETY: waitpid on our own child; status is a valid out-pointer.
    let r = unsafe { libc::waitpid(pid, &mut status, 0) };
    if r < 0 {
        return -1;
    }
    status
}

// ─────────────────────────── supervisor-side helpers ───────────────────────────

/// A worker the spawner created, as the supervisor sees it: the pid (to correlate
/// its later exit) and the control socket to proxy the PAM conversation over.
pub struct SpawnedWorker {
    pub pid: libc::pid_t,
    pub control: UnixStream,
}

/// Ask the spawner for a worker: send the request, read the pid, receive the control
/// fd. The returned control socket is the supervisor's end — the greeter socket is
/// never sent here, so the worker (the spawner's child) can never reach a greeter byte.
pub fn request_worker(sock: &mut UnixStream) -> io::Result<SpawnedWorker> {
    sock.write_all(&[REQ_SPAWN])?;
    let mut tag = [0u8; 1];
    sock.read_exact(&mut tag)?;
    match tag[0] {
        EV_STARTED => {
            let mut pid_buf = [0u8; 4];
            sock.read_exact(&mut pid_buf)?;
            let pid = libc::pid_t::from_le_bytes(pid_buf);
            let control_fd = fdpass::recv_fd(sock)?;
            Ok(SpawnedWorker {
                pid,
                control: UnixStream::from(control_fd),
            })
        }
        EV_ERROR => Err(io::Error::other("spawner failed to create a worker")),
        other => Err(io::Error::other(format!(
            "spawner sent an unexpected reply tag {other:#x}"
        ))),
    }
}

/// Block until the spawner reports the current worker's exit, returning its raw wait
/// status. This is what the supervisor's session-wait uses instead of `waitpid` now
/// that the worker is the spawner's child, not the supervisor's.
pub fn wait_worker_exit(sock: &mut UnixStream) -> io::Result<i32> {
    loop {
        let mut tag = [0u8; 1];
        sock.read_exact(&mut tag)?;
        if tag[0] != EV_EXITED {
            continue; // skip a stray tag rather than desync
        }
        let mut buf = [0u8; 8];
        sock.read_exact(&mut buf)?;
        let status = i32::from_le_bytes(buf[4..8].try_into().unwrap());
        return Ok(status);
    }
}

/// Extension: the raw fd of a `UnixStream` without consuming it (for `SCM_RIGHTS`).
trait AsRawExt {
    fn as_raw(&self) -> std::os::fd::RawFd;
}
impl AsRawExt for UnixStream {
    fn as_raw(&self) -> std::os::fd::RawFd {
        std::os::fd::AsRawFd::as_raw_fd(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in "worker": a child that holds the far end of a control socketpair
    /// and exits 0 as soon as that socket hits EOF — exactly the lifecycle shape of
    /// the real worker, but with no PAM/session. Returns (child_pid, supervisor_end).
    fn stand_in_worker() -> io::Result<(libc::pid_t, UnixStream)> {
        let (supervisor_end, child_end) = UnixStream::pair()?;
        // SAFETY: fork; the child does only async-signal-safe fd work before _exit.
        match unsafe { libc::fork() } {
            -1 => Err(io::Error::last_os_error()),
            0 => {
                drop(supervisor_end);
                let mut ce = child_end;
                let mut buf = [0u8; 64];
                loop {
                    match ce.read(&mut buf) {
                        Ok(0) | Err(_) => break, // EOF: the supervisor released control
                        Ok(_) => {}
                    }
                }
                unsafe { libc::_exit(0) };
            }
            pid => {
                drop(child_end);
                Ok((pid, supervisor_end))
            }
        }
    }

    /// End-to-end of the spawner mechanism (no real worker): fork the spawner, ask
    /// it for a worker, prove the passed control fd is live, then release it and
    /// confirm the spawner reaps the worker and reports its exit.
    #[test]
    fn spawns_a_worker_passes_its_control_fd_and_reports_exit() {
        let mut sup = fork_spawner(stand_in_worker).expect("fork spawner");

        let worker = request_worker(&mut sup).expect("request worker");
        assert!(worker.pid > 0, "got a real pid");

        // The control fd is live: a write to it succeeds (the stand-in drains it).
        let mut control = worker.control;
        control.write_all(b"ping").expect("control fd is writable");

        // Release the control socket → the stand-in sees EOF → exits 0.
        drop(control);

        let status = wait_worker_exit(&mut sup).expect("exit report");
        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "worker should exit cleanly, raw status {status}"
        );
    }
}
