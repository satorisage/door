# DECISION-0016 — Supervisor seccomp filter via seccompiler, log-before-enforce

**Status:** Binding
**Date:** 2026-07-01
**Ratified:** 2026-07-01
**Project:** door
**Relates to / extends:** D-0015 (pre-forked spawner — this is the Tier 3 build it
enabled; the supervisor is the confined process, the spawner lineage stays free),
D-0003 (hardened seam / TCB defaults — this bounds the TCB's syscall power), D-0005
(per-login worker — its lineage is a child of the un-sandboxed spawner, so it does
not inherit this filter). **Source:** `.agent/IDEAS/2026-06-30-seccomp-landlock-scope.md`,
`.agent/CHECKINS/2026-07-01-tier3-seccomp-open.md`.

## Context

D-0015 split the daemon into a **supervisor** (long-lived, root, parses untrusted
pre-auth input: IPC, greeter framing, seat/VT/DRM) and a **pre-forked spawner**
(forked once at startup, before any sandbox, whose descendants — the per-login
worker and the user's desktop session — must run with a full syscall set). Tier 3
increment 1 (greeter routed through the spawner, HEAD `86739f6`) is
hardware-validated on `genny`, so the greeter compositor no longer forks off the
supervisor — the supervisor is finally confinable without leaking a filter into the
desktop.

What D-0015 left open and this decision settles: **which mechanism builds the
filter, and the concrete rollout.** No seccomp code exists yet — `hardening.rs` is
Tier 0 only (`NO_NEW_PRIVS` + `PR_SET_DUMPABLE`), and there is no seccomp
dependency in `doord/Cargo.toml`.

## Decision

Build the supervisor seccomp filter with the **`seccompiler`** crate
(pure-Rust BPF compiler, no libseccomp C system-library dependency), applied
**supervisor-only, after `fork_spawner`**, with a **log-before-enforce** rollout.

### Library — `seccompiler`

- Pure-Rust: compiles a `SeccompFilter` rule set to a classic-BPF program and installs
  it via `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER)`. No C `libseccomp` build/runtime
  dependency added to genny's supply chain.
- Rejected: **`seccomp` (libseccomp FFI)** — battle-tested but adds a C system-lib
  build dep; **raw `libc` prctl + hand-written BPF** — zero new deps but we would own
  all arch/syscall-number tedium and the error-prone jump arithmetic seccompiler exists
  to eliminate. The allowlist we need is small and static; seccompiler is the right
  altitude.

### Placement — supervisor-only, strictly after the spawner fork

Startup ordering is load-bearing and already fixed by D-0015: **fork spawner → bind
listener → apply sandbox → serve.** The seccomp install is the "apply sandbox" step,
added in `hardening.rs` and called from the supervisor path only, **after**
`fork_spawner` returns. Applying it before the fork would re-introduce the D-0015
inheritance bug (the desktop would inherit the filter). This ordering keeps its
existing test and gains a seccomp-specific assertion that the spawner is forked first.

### Rollout — log before enforce (default action staged)

Per D-0015's log-only clause, a missed syscall under a killing filter is a **login
lockout**, so the default action ships in two stages:

1. **`SCMP_ACT_LOG`** (increment 2a→2b): unknown syscalls are *logged, not blocked*.
   Boot genny, exercise login → logout → greeter-recycle → re-login, then
   `journalctl | grep -i seccomp` to collect the real denied-syscall set and widen the
   allowlist until the audit log is clean under normal operation.
2. **Enforce** (increment 3): flip the default action to `SCMP_ACT_ERRNO(EPERM)` (not
   `KILL` — an errno is debuggable and a softer failure than a killed supervisor) once
   the log run is quiet across a full cycle.

### Control surface

- A single env flag **`DOORD_SECCOMP`** selects the mode: unset/`off` → no filter (Tier 0
  only, current behavior); `log` → install with `SCMP_ACT_LOG` default; `enforce` →
  install with `SCMP_ACT_ERRNO`. Lets a genny boot pick the stage without a rebuild and
  keeps the filter reversible while Tier 3/4 are proven, mirroring the `DOORD_SPAWNER`
  retention pattern.
- The **`DOORD_NO_SANDBOX=1`** recovery kill-switch from D-0015 forces `off` regardless,
  for lockout recovery.

## Alternatives considered

- **libseccomp FFI (`seccomp` crate).** Rejected — a C system-lib dependency for a small
  static allowlist we can express directly; no resolver features we need.
- **Raw `libc` prctl + hand-rolled BPF.** Rejected — no new dep, but reimplements what
  seccompiler does correctly (arch checks, syscall-number tables, jump offsets); higher
  bug surface on a security-critical filter.
- **Skip log stage, ship enforce directly.** Rejected — a missed syscall is a login
  lockout on the real machine; the log pass is the only way to enumerate the true
  syscall set of the supervisor's steady state (PAM proxy relay, seat/VT/DRM ioctls,
  epoll/accept loop) without guessing.
- **`SCMP_ACT_KILL` as the enforce action.** Rejected in favor of `ERRNO(EPERM)` — a
  killed supervisor drops the greeter and every future login; an errno lets a stray
  syscall fail locally and surface in logs without taking down the seat.

## Consequences

- `seccompiler` added to `doord/Cargo.toml` (+ workspace dep entry).
- `hardening.rs` gains a `apply_seccomp(mode)` step (the allowlist + default-action
  selection); called supervisor-only after `fork_spawner`. `apply_baseline` (Tier 0)
  is unchanged and still runs at startup.
- New `DOORD_SECCOMP={off|log|enforce}` env flag; `DOORD_NO_SANDBOX=1` overrides to off.
- The allowlist is **empirically derived** from a genny `SCMP_ACT_LOG` boot, not guessed
  — increment 2b is a required hardware step, and what gets dropped/added is recorded.
- Threat-model update: the supervisor's confined syscall surface documented in
  `.agent/SECURITY/` (session-spawn threat model), noting the spawner/worker/session
  lineage stays unconfined by design (D-0015 exclusion).
- Unblocks Tier 4 (Landlock paths) on the same post-fork supervisor step.
- Enforcement action deliberately `EPERM`, not `KILL`, so a filter gap degrades to a
  logged local failure rather than a seat-wide login outage.

## Ratification note

Critical (privilege boundary / login path per PROJECT-STATE), so pre-ratified before
build per the lifecycle rule. The log-before-enforce, post-fork placement, and
kill-switch envelope were already ratified in D-0015; this decision fixes the library
(`seccompiler`) and the concrete rollout mechanics within that envelope. Build lands
across increments 2a (log filter) → 2b (genny log run + allowlist tuning) → 3 (enforce),
each carrying its `.agent/` delta in the shipping commit.
