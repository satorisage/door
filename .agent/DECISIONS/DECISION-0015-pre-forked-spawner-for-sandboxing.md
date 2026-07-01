# DECISION-0015 — Pre-forked spawner so the supervisor can sandbox itself

**Status:** Binding
**Date:** 2026-06-30
**Ratified:** 2026-06-30
**Project:** door
**Relates to / extends:** D-0005 (per-login session worker — its worker role is
unchanged; only the worker's *parent* changes), D-0003 (the seam is the trust
boundary — preserved), D-0009 (session lifetime tied to doord — preserved).
**Enables:** M5 seccomp (Tier 3) + Landlock (Tier 4). **Source:**
`.agent/IDEAS/2026-06-30-seccomp-landlock-scope.md`.

## Context

M5 Tier 1 (systemd unit hardening) hit a hard wall, and it is architectural, not
cosmetic. seccomp filters and Landlock rulesets are **inherited across `fork` and
preserved across `execve`** — by design. Under D-0005 the per-login worker (and
therefore the user's session/compositor it `execve`s) is forked **from the daemon,
after startup**, in the daemon's own mount namespace. So **any sandbox the daemon
applies to itself is inherited by the user's desktop** — a `RestrictAddressFamilies`
would strip the session's network, a `SystemCallFilter` would kill its syscalls, a
Landlock ruleset would confine its filesystem. Verified: `apply_baseline` runs at
startup; the worker is re-exec'd per-login inside the accept loop; the session
shares the daemon's mount namespace.

Consequently the daemon **cannot self-sandbox** without breaking every login. Only
a narrow "neither doord nor a desktop ever does this" subset is safe at the unit
level (shipped in Tier 1). To bound the blast radius of a compromise in the
root-privileged, pre-auth-reachable code (IPC parse, PAM relay, seat management) —
the whole point of M5 — **session-spawn must happen outside the sandboxed process
lineage.** Nothing else unlocks real kernel-level confinement of the TCB.

## Decision

Introduce a **pre-forked spawner** process, forked **once at startup, before any
sandbox is applied**, that owns worker creation. The daemon splits into two roles:

- **Supervisor** (the former main daemon): binds the listener, `accept`s, does the
  `SO_PEERCRED` gate, all greeter wire-protocol framing, and seat/VT/DRM management
  (`free_seat`, VT restore). After forking the spawner and binding the listener,
  **it applies seccomp + Landlock to itself** (Tier 3/4), then serves.
- **Spawner**: a minimal, long-lived helper, forked before the sandbox, that on the
  supervisor's request (over a private control socketpair) forks/re-execs the
  **per-login session worker of D-0005 — unchanged in role**: the worker still owns
  the entire PAM transaction, is the logind session leader, spawns the session,
  waits, closes the session, and exits.

Because the per-login worker is now a child of the **un-sandboxed spawner** (not the
sandboxed supervisor), neither the worker's PAM modules nor the `execve`'d session
inherits the supervisor's seccomp/Landlock. **The desktop runs with a full
profile.** The supervisor — the long-lived, root, untrusted-input-facing process —
is the one that gets confined.

### What is preserved (unchanged from D-0005 / D-0003)

- The worker owns the full PAM transaction and logind leadership; auth→session
  continuity (keyring unlock) is intact.
- **The greeter socket stays solely in the supervisor.** The spawner and worker
  never read a greeter byte: the greeter fd is `O_CLOEXEC` and the spawner is
  forked without it. The PAM conversation is still proxied by the supervisor
  (Prompt/Reply relay), exactly as D-0005 specifies.
- Identity binding: the session runs as the PAM-authenticated user; `Start` still
  carries only a `session_id`, gated on a real auth success.
- Session lifetime + seat teardown (D-0009) unchanged.

### The spawner's trust surface (why un-sandboxed is acceptable)

The spawner is small and speaks **only a tiny private codec to the supervisor over
a socketpair** — it parses no untrusted input, holds no greeter fd, and its sole
power is "fork a worker on request." *When* a worker may be forked is still gated
by the supervisor (only after a peercred-authorized greeter drives a completed
auth). It stays un-sandboxed by necessity — its descendants (worker, session) must
run free — but it is reachable only from the sandboxed supervisor and does nothing
an attacker can steer. This is an explicit, bounded exclusion (honest-bounds), not
an oversight.

## Alternatives considered

- **Keep re-exec-from-supervisor, apply the sandbox "loosely."** A filter permissive
  enough for a full desktop is no protection; and a filter cannot be relaxed after
  the fact, so any worker forked post-sandbox inherits it. Rejected — it is the
  problem, not a fix.
- **systemd unit hardening only (Tier 1).** Same inheritance; only the narrow
  session-safe subset works. Already shipped, insufficient on its own.
- **A separate socket-activated `doord-spawn.service` for session spawn.** Achieves
  the lineage split, but crosses a service boundary per login, adds a unit + socket
  + activation race, and loosens the tight lifecycle coupling D-0009 relies on
  (doord owning the session's lifetime). The in-process pre-forked spawner is
  lighter and keeps that coupling.
- **Do not sandbox the daemon.** Rejected — leaves the root TCB syscall/FS-unbounded,
  abandoning the M5 goal.

## Consequences

- New long-lived **spawner** process/mode (`main.rs` dispatch, like the worker
  re-exec but forked once at startup and persistent). `pam.rs` daemon side routes
  worker creation through the supervisor→spawner control socket instead of forking
  the worker directly.
- **fd-passing (the fiddly part):** the supervisor holds the greeter socket
  (untrusted) and the daemon-end of the worker control socketpair; the worker is
  forked by the spawner. The control socketpair's worker-end must reach the worker
  via `SCM_RIGHTS` through the supervisor→spawner channel (or the spawner mints the
  pair and passes the supervisor-end back). Designed so the supervisor keeps
  proxying the PAM conversation while never handing the worker a greeter fd.
- **Startup ordering is load-bearing:** fork spawner → bind listener → apply sandbox
  → serve. Applying the sandbox before the spawner fork would re-introduce the
  inheritance bug; this ordering gets a test.
- `apply_baseline` (`NO_NEW_PRIVS`, non-dumpable) stays; the seccomp/Landlock step is
  added **after** the spawner fork, supervisor-only.
- A **`DOORD_NO_SANDBOX=1` kill-switch** for recovery, and a **log-only rollout**
  (`SCMP_ACT_LOG` before `KILL`) for the seccomp phase — a missed syscall under KILL
  is a login lockout.
- Threat-model update: a new spawner section in `session-spawn-threat-model.md` (its
  inputs, its power, the un-sandboxed exclusion) and confirmation that the greeter
  socket remains supervisor-only.
- Unblocks Tier 3 (seccomp allowlist) and Tier 4 (Landlock paths) on the supervisor.

## Ratification note

Material, DECISION-gated (touches the privilege architecture the session-spawn
threat model rests on). Not yet built. Ratify before implementing Tier 2/3/4; the
build lands with a same-commit `ratify D-0015` per the lifecycle rule.
