# DECISION-0004 — logind session registration via pam_systemd

**Status:** Binding
**Date:** 2026-06-25
**Ratified:** 2026-06-25
**Project:** door

## Context

M2's last task. Auth and the privilege-drop spawn are done and live-confirmed,
but the spawned process is **not a logind session**: no `XDG_SESSION_*`, no
seat/VT registration, no controlling tty — the child inherits the daemon's stdio
(which is exactly what made the privilege drop demonstrable, and exactly what the
session-spawn threat model deferred as N1/N2). This decision closes that gap.

At the gating fork — *how does door register the logind session* — Stephen chose
(structured selection, 2026-06-25) the **pam_systemd** path over a direct logind
`CreateSession` D-Bus call. This decision records that path, its structural
consequences for the PAM seam, and the one remaining open Critical sub-fork (the
process-leadership model).

This task touches the **session-handoff contract** and **VT/seat ownership** —
both Critical per the SCOPE rubric — so it is ratified before implementation.

## Decision

### Registration mechanism

- **R1 — Register via `pam_systemd`, not a direct D-Bus call.** door drives the
  session through PAM session management; the `doord` PAM stack includes
  `pam_systemd.so` in its session phase, which calls logind's `CreateSession`
  *for us*. Rationale: it is what every production seat manager (greetd / gdm /
  sddm / lightdm) does; it is robust against logind's semi-private,
  version-sensitive `CreateSession` API; and — decisively — it needs **no D-Bus
  client in the daemon**, so the *single-threaded-at-fork* invariant `spawn.rs`
  relies on (D-0003, the `pre_exec` privilege drop) is preserved: no async
  runtime is pulled into the TCB.

### PAM transaction lifetime

- **R2 — One PAM transaction spans the whole login.** Today `pam.rs` opens and
  drops a `Context` *inside* `authenticate()`. For `pam_systemd` the same
  `Context` must live from `authenticate()` → `acct_mgmt()` → `open_session()` →
  fork/exec → `wait` → close. `Context::open_session()` (pam-client 0.5) performs
  `setcred(ESTABLISH)` + `pam_open_session` + `setcred(REINITIALIZE)` and returns
  a `Session` guard whose `Drop`/`close` does `pam_close_session` +
  `setcred(DELETE)`. The daemon holds that guard alive across the session's
  lifetime and drops it when the child exits (same EUID 0 closes as opened).

### Session environment

- **R3 — Session env = sanitized allowlist ∪ PAM env.** After `open_session`,
  `Session::envlist()` carries logind's contributions (`XDG_SESSION_ID`,
  `XDG_RUNTIME_DIR=/run/user/<uid>`, possibly `DBUS_SESSION_BUS_ADDRESS`). The
  child environment becomes `sanitized_env(target)` **merged** with the PAM
  envlist (PAM values win for the `XDG_*` keys; the allowlist still defines
  `HOME`/`USER`/`LOGNAME`/`SHELL`/`PATH` and still bars `LD_*`). This relaxes
  M2's "`env_clear` + allowlist only" — no daemon env leaks, but PAM's session
  vars are now admitted *by construction*. Supersedes the strict-replace wording
  in the spawn module's doc comment.

### Session inputs and tty

- **R4 — PAM session inputs set before `open_session`** via `putenv`:
  `XDG_SEAT`, `XDG_VTNR`, `XDG_SESSION_TYPE` (`wayland`|`x11`, from the chosen
  session's kind), `XDG_SESSION_CLASS=user`, `XDG_SESSION_DESKTOP=<session id>`.
  door sources seat/vtnr from its **own** logind session environment (door runs
  as a logind session on a dedicated VT, launched by its unit), falling back to
  config (`DOORD_SEAT`/`DOORD_VTNR`) for dev/test. The greeter never chooses
  them (identity-binding, same posture as `Start` carrying only a session id).
- **R5 — Controlling tty + `setsid` in the child.** After privdrop, before
  exec, the child `setsid()`s (becomes session leader), opens `/dev/tty<vtnr>`,
  `TIOCSCTTY` to acquire it as the controlling tty, and dups it onto stdio — so
  the session no longer inherits the daemon's stdio. Greeter and session share
  door's VT (greetd-style), so no cross-VT `VT_ACTIVATE` switch is needed for
  single-seat v1.

### Lifecycle

- **R6 — Clean open→run→close cycle.** On session exit the daemon drops the
  `Session` guard → `pam_close_session` + `setcred(DELETE)` → logind tears the
  session down; then `pam_end` via `Context` drop. Respawn/backoff/re-greet
  (threat N2) stays deferred to a follow-up; this task lands the single clean
  cycle.

## Process-leadership model (ratified: A — **superseded by D-0005, 2026-06-26**)

> **Superseded by D-0005.** The first live run showed plan A registers only one
> logind session per daemon lifetime (`pam_systemd` migrates the long-lived
> daemon into the session scope; the session never closes and a second login
> cannot register). D-0005 moves the leader to a per-login worker process (plan B).
> The rest of this decision (pam_systemd, env merge R3, seat/VT R4, VT/tty R5)
> still stands. The original weighing is kept below for the record.

**Ratified 2026-06-25 → (A) Daemon-holds-context.** door is single-seat and
sequential; the daemon owns the PAM `Context` across auth and session and is the
logind session leader. (B) is revisited in M5 only if daemon↔session cgroup
isolation is judged necessary. The two options as weighed:

- **(A) Daemon-holds-context** *(recommended for v1)*. The daemon — which
  already ran auth — calls `open_session` and forks the session child; the
  daemon is the PAM caller and thus the logind session **leader**, and the child
  runs in the daemon's cgroup scope. Fits door's strictly-sequential,
  one-session-at-a-time model (the daemon blocks in `wait()` for the whole
  session, serving no other greeter). Smallest change. Cost: the logind session
  scope contains the daemon process, not just the user session — weaker cgroup
  isolation between TCB and session. Acceptable: door **is** the TCB; the modeled
  attacker is the greeter, a separate process.
- **(B) Per-login worker subprocess** *(greetd model)*. At `BeginAuth` the daemon
  forks a per-login worker that owns the PAM `Context`, runs the conversation
  proxied to the greeter through the daemon, then on `Start` calls
  `open_session` and **becomes** (execs into) the session — so the session
  subtree alone is the logind scope and the daemon stays out of it. Cleaner
  leadership/isolation; significantly larger refactor (PAM moves out of the
  daemon; the conversation is proxied over a pipe). Better fit only if door later
  serves concurrent seats — out of v1 scope.

**Recommendation: (A) for v1.** Honest bounds: door is single-seat and
sequential; (A) meets the milestone DoD ("the chosen session runs as the
authenticated user, wired into the seat/VT via logind") with the least new TCB
surface. Revisit (B) in M5 hardening if daemon↔session cgroup isolation is judged
necessary.

## Alternatives considered

- **Direct logind `CreateSession` D-Bus.** Rejected at the path fork: semi-private
  positional API, pulls a D-Bus/async client into the TCB, reimplements what
  `pam_systemd` already does.
- **`open_pseudo_session`.** Establishes credentials without a real logind
  session — defeats the purpose (no seat/VT/runtime-dir).

## Consequences

- `pam.rs` restructures: the `Authenticator` / `SessionLauncher` split folds into
  a per-login PAM transaction owning the `Context` across auth **and** session;
  the in-process test seam (`ScriptedAuthenticator` / `RecordingLauncher`) is
  preserved by abstracting the transaction, not by re-opening PAM per phase.
- `spawn.rs`: `build_command`'s `env_clear` + allowlist becomes allowlist ∪ PAM
  envlist (R3); `pre_exec` gains `setsid` + controlling-tty acquisition (R5)
  alongside the existing privilege drop.
- A `/etc/pam.d/doord` **session** stack including `pam_systemd.so` (packaging;
  the dev service file from the live run already exists).
- A threat-model addendum promoting **N1** (and partially **N2**) from "deferred"
  to "modeled": handoff races, VT/tty confusion, the daemon-in-scope consequence
  of (A), close-session on crash.
- Protocol: **no wire change** required (`Start` already carries only a session
  id); `PROTOCOL_VERSION` unchanged.
- Live DoD for the task: `loginctl` shows the spawned session on `seat0` at the
  VT, running as the authenticated user, and a real Wayland session comes up —
  verified on hardware (with door launched on a real VT, not from a dev terminal).
