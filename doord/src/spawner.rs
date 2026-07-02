//! The pre-forked spawner (M5 sandbox groundwork).
//!
//! seccomp/Landlock are inherited across `fork` and preserved across `execve`, so a
//! sandbox the supervisor applies to itself would also land on the children it
//! forks and the programs they `execve` — the user's session, and the greeter's
//! compositor. To let the supervisor confine itself without confining the desktop
//! *or* the greeter compositor, both must be created **outside** the supervisor's
//! sandboxed lineage.
//!
//! This spawner is forked once at startup, *before* any sandbox. It owns child
//! creation: on the supervisor's request it forks/re-execs either the per-login
//! **session worker** (D-0005) or the **greeter worker** (D-0008), hands that
//! child's control-socket fd back to the supervisor with `SCM_RIGHTS`
//! ([`crate::fdpass`]), and reaps it when it exits. Because these children are the
//! spawner's — not the sandboxed supervisor's — neither PAM, the session, nor the
//! greeter compositor inherits the supervisor's sandbox.
//!
//! Exit is observed by the *supervisor* directly, as EOF on the control fd it was
//! handed: each child holds its control fd open for its whole life and closes it on
//! exit. The spawner's `waitpid` runs purely to reap (no zombies); the supervisor
//! never depends on its timing.
//!
//! Concurrency: the greeter worker and the session worker are **alive at the same
//! time** — the session worker is created when the greeter connects (it owns the
//! PAM auth conversation) and runs alongside the still-displayed greeter until the
//! login handoff. So the spawner must serve new requests while children are still
//! running: it never blocks reaping one child, and instead sweeps all exited
//! children non-blockingly (woken by `SIGCHLD`, which interrupts its request `poll`).
//!
//! Trust surface: the spawner parses no untrusted input — it speaks only this tiny
//! codec to the supervisor over a socketpair, holds no greeter fd, and its only
//! power is "fork a child on request." *When* that is allowed is still gated by the
//! supervisor (a session worker only after a peercred-authorized greeter drives a
//! completed auth).

use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;

use crate::fdpass;

/// Supervisor → spawner: "fork a session worker."
const REQ_SPAWN_WORKER: u8 = b'S';
/// Supervisor → spawner: "fork a greeter worker."
const REQ_SPAWN_GREETER: u8 = b'G';
/// Spawner → supervisor: child started; a 4-byte LE pid follows, then an
/// `SCM_RIGHTS` control fd.
const EV_STARTED: u8 = b'R';
/// Spawner → supervisor: the spawn failed; no fd follows.
const EV_ERROR: u8 = b'E';

/// Which child the spawner should create.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    /// The per-login session worker (D-0005): owns the PAM transaction and is the
    /// logind session leader.
    Worker,
    /// The greeter worker (D-0008): opens a passwordless greeter session and runs
    /// the greeter compositor.
    Greeter,
}

/// How the spawner creates a child: returns `(child_pid, supervisor_end_of_control)`.
/// Production injects a `pam`-side fork; tests inject a stand-in.
pub type SpawnFn = fn(SpawnKind) -> io::Result<(libc::pid_t, UnixStream)>;

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
            // spawner (which could otherwise keep spawning children).
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

/// The spawner's serve loop. Reaps any exited children, waits for the next request
/// (a child exit's `SIGCHLD` interrupts the wait so reaping stays prompt), forks the
/// requested child via `spawn`, and passes its control fd back to the supervisor.
/// Returns on EOF (the supervisor is gone).
pub fn run_spawner(mut sock: UnixStream, spawn: SpawnFn) {
    install_sigchld_interrupt();
    let fd = sock.as_raw();
    loop {
        // Concurrent model: greeter + session worker can be alive at once, so never
        // block reaping one — sweep every child that has already exited. The
        // supervisor learns exit from control-fd EOF; this only prevents zombies.
        reap_exited();

        // Wait for the next request. A child exit delivers SIGCHLD, whose no-op
        // handler (installed without SA_RESTART) interrupts this poll so we loop
        // back and reap promptly rather than leaving a zombie until the next request.
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd, count 1; poll writes only revents.
        let ready = unsafe { libc::poll(&mut pfd, 1, -1) };
        if ready < 0 || pfd.revents & libc::POLLIN == 0 {
            // EINTR (a SIGCHLD) or no readable request: loop to reap and re-poll.
            continue;
        }

        let mut tag = [0u8; 1];
        match sock.read_exact(&mut tag) {
            Ok(()) => {}
            // Supervisor closed the channel — nothing more to do.
            Err(_) => return,
        }
        let kind = match tag[0] {
            REQ_SPAWN_WORKER => SpawnKind::Worker,
            REQ_SPAWN_GREETER => SpawnKind::Greeter,
            // Unknown request; ignore rather than die (defensive).
            _ => continue,
        };

        let (pid, control) = match spawn(kind) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("doord: spawner: could not create a {kind:?} child: {e}");
                let _ = sock.write_all(&[EV_ERROR]);
                continue;
            }
        };

        // Report the child + hand its control fd to the supervisor.
        let mut header = [0u8; 5];
        header[0] = EV_STARTED;
        header[1..].copy_from_slice(&pid.to_le_bytes());
        let handed =
            sock.write_all(&header).is_ok() && fdpass::send_fd(&sock, control.as_raw()).is_ok();
        drop(control); // the supervisor owns the control fd now; the spawner does not
        if !handed {
            // The supervisor is gone; the child we just forked is reaped by init.
            return;
        }
    }
}

/// Reap every child that has already exited, without blocking. `WNOHANG` returns 0
/// when none are ready and -1 (`ECHILD`) when there are no children; both end the
/// sweep. The supervisor detects exit via control-fd EOF independently — this only
/// keeps zombies from accumulating.
fn reap_exited() {
    let mut status: libc::c_int = 0;
    // SAFETY: waitpid over our own children; status is a valid out-pointer.
    while unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) } > 0 {}
}

/// A no-op SIGCHLD handler whose only purpose is to interrupt the spawner's blocking
/// `poll` (it is installed without `SA_RESTART`) so the loop wakes and reaps. Reaping
/// happens in the loop, not here, keeping the handler trivially async-signal-safe.
extern "C" fn on_sigchld(_sig: libc::c_int) {}

/// Install [`on_sigchld`] for SIGCHLD with no `SA_RESTART`, so a child exit
/// interrupts the request `poll`.
fn install_sigchld_interrupt() {
    // SAFETY: a zeroed sigaction pointed at an async-signal-safe (empty) handler,
    // registered for SIGCHLD; sa_flags = 0 means no SA_RESTART, so poll is interrupted.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_sigchld as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        libc::sigaction(libc::SIGCHLD, &action, std::ptr::null_mut());
    }
}

// ─────────────────────────── supervisor-side handle ───────────────────────────

/// A child the spawner created, as the supervisor sees it: the pid (to signal it /
/// correlate its exit) and the control socket to observe it (and, for a session
/// worker, proxy the PAM conversation) over.
pub struct SpawnedChild {
    pub pid: libc::pid_t,
    pub control: UnixStream,
}

/// The supervisor's handle to the pre-forked spawner. Owns the single control
/// socket; the supervisor asks it for a greeter worker (at greet time) or a session
/// worker (when a greeter connects) over that one channel. `RefCell` because the
/// request methods take `&self` — the supervisor borrows the handle from both the
/// greet step and the per-connection login — and logins are strictly serial.
pub struct Spawner {
    control: RefCell<UnixStream>,
}

impl Spawner {
    /// Wrap the supervisor's end of the spawner control socket.
    pub fn new(control: UnixStream) -> Self {
        Spawner {
            control: RefCell::new(control),
        }
    }

    /// Ask the spawner for a session worker.
    pub fn request_worker(&self) -> io::Result<SpawnedChild> {
        self.request(REQ_SPAWN_WORKER)
    }

    /// Ask the spawner for a greeter worker.
    pub fn request_greeter(&self) -> io::Result<SpawnedChild> {
        self.request(REQ_SPAWN_GREETER)
    }

    /// Send one spawn request, read the pid, receive the control fd. The returned
    /// control socket is the supervisor's end — the greeter socket is never sent
    /// here, so the spawner's children can never reach a greeter byte.
    fn request(&self, req: u8) -> io::Result<SpawnedChild> {
        let mut sock = self.control.borrow_mut();
        sock.write_all(&[req])?;
        let mut tag = [0u8; 1];
        sock.read_exact(&mut tag)?;
        match tag[0] {
            EV_STARTED => {
                let mut pid_buf = [0u8; 4];
                sock.read_exact(&mut pid_buf)?;
                let pid = libc::pid_t::from_le_bytes(pid_buf);
                let control_fd = fdpass::recv_fd(&sock)?;
                Ok(SpawnedChild {
                    pid,
                    control: UnixStream::from(control_fd),
                })
            }
            EV_ERROR => Err(io::Error::other("spawner failed to create a child")),
            other => Err(io::Error::other(format!(
                "spawner sent an unexpected reply tag {other:#x}"
            ))),
        }
    }
}

/// Extension: the raw fd of a `UnixStream` without consuming it (for `SCM_RIGHTS`
/// and `poll`).
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

    /// A stand-in child: a process that holds the far end of a control socketpair
    /// and exits 0 as soon as that socket hits EOF — the lifecycle shape of a real
    /// worker/greeter, but with no PAM/session. Returns (child_pid, supervisor_end).
    fn stand_in(_kind: SpawnKind) -> io::Result<(libc::pid_t, UnixStream)> {
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

    /// The spawner must serve a second request while the first child is still alive
    /// — the concurrent model (greeter + session worker coexist). The old serial
    /// spawner would have blocked reaping the first and deadlocked here. Request a
    /// greeter-kind and a worker-kind child, prove both control fds are live at the
    /// same time, then release them (which lets the stand-ins exit and be reaped);
    /// a third request still succeeds, proving the loop served through their
    /// lifetimes and their reaping.
    #[test]
    fn serves_concurrent_children_of_both_kinds() {
        let sup = Spawner::new(fork_spawner(stand_in).expect("fork spawner"));

        let greeter = sup.request_greeter().expect("request greeter");
        let worker = sup
            .request_worker()
            .expect("request worker while the greeter is still alive");
        assert!(greeter.pid > 0 && worker.pid > 0, "both got real pids");
        assert_ne!(greeter.pid, worker.pid, "distinct children");

        // Both control fds are live simultaneously (a write on each succeeds).
        let mut gc = greeter.control;
        let mut wc = worker.control;
        gc.write_all(b"ping").expect("greeter control fd is live");
        wc.write_all(b"ping").expect("worker control fd is live");

        // Release both → the stand-ins see EOF and exit; the spawner reaps them.
        drop(gc);
        drop(wc);

        let third = sup
            .request_worker()
            .expect("request a third child after the first two exit");
        assert!(third.pid > 0, "spawner kept serving through reaping");
    }
}
