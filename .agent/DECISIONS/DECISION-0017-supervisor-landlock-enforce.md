# DECISION-0017 — Supervisor Landlock path sandbox via the landlock crate, enforce-only (M5 Tier 4)

**Status:** Binding
**Reversibility:** cheap (flag-gated, default-off; `DOORD_NO_SANDBOX=1` kill-switch)
**Date:** 2026-07-02
**Ratified:** 2026-07-02
**Project:** door
**Relates to / extends:** D-0016 (supervisor seccomp — this is the sibling path-sandbox
on the same post-`fork_spawner` supervisor step; seccomp bounds *syscalls*, Landlock
bounds *paths*), D-0015 (pre-forked spawner — the enabling split; the confined process
is the supervisor, the spawner/worker/session lineage stays unconfined by design),
D-0003 (hardened seam / TCB defaults). **Source:**
`.agent/IDEAS/2026-06-30-seccomp-landlock-scope.md` (Tier 4).

## Context

D-0015 moved session and greeter spawning off the supervisor's lineage (onto the
pre-forked spawner), and D-0016 used that to confine the supervisor's *syscalls*
(seccomp, now shipping `enforce`). Tier 4 confines the same supervisor's *filesystem
reach*: a compromised supervisor should not be able to read `/home`, write
`/etc/shadow`, or touch arbitrary user data outside the small set of subtrees it needs
(the IPC socket dir, session-discovery dirs, `/proc` for the seat scan, DRM/VT devices).

What this decision settles: the **library**, the **rollout shape** (which differs from
seccomp's because Landlock has no permissive mode), and the **ABI floor**.

The central constraint is that **Landlock has no `SCMP_ACT_LOG` equivalent** — no
observe-without-breaking mode. Any path outside the ruleset is blocked the moment the
ruleset is active. (Kernel 6.15+ audits denials, but the denial still blocks —
diagnostics while enforcing, not permissive operation.) So D-0016's log-before-enforce
stage cannot transfer, and the rollout must reach a correct allowlist another way.

## Decision

Build the supervisor filesystem sandbox with the **`landlock`** crate (pure-Rust,
best-effort ABI compatibility), applied **supervisor-only, after `fork_spawner`**,
**enforce-only** behind a default-off `DOORD_LANDLOCK` flag, with the path allowlist
seeded from the code's touch-points + the Tier-3 `openat` log data and **tuned on genny
before the shipped unit flips**.

**(part 1 — library).** The `landlock` crate: a pure-Rust safe wrapper over the Landlock
syscalls with best-effort compatibility (`CompatLevel::BestEffort`), so an older kernel
(or one without Landlock) degrades to `RulesetStatus::NotEnforced`/`PartiallyEnforced`
instead of aborting startup — mirroring the seccompiler choice in D-0016 (no
C-`libseccomp`-style system dependency). Rejected: raw `libc` + hand-rolled
`landlock_*` syscalls — reimplements the crate's ABI-compat handling on a
security-critical path.

**(part 2 — placement).** Landlock rulesets are inherited across `fork` and preserved
across `execve` — the same constraint as seccomp. Install order is load-bearing:
**fork spawner → bind listener → apply_seccomp → apply_landlock → serve.** Applied before
the fork (or on the direct in-lineage path, where the supervisor itself forks the
session) the ruleset would confine the desktop and break every login; `main.rs` gates it
on `spawner.is_some()` exactly as it does seccomp.

**(part 3 — rollout, enforce-only).** The flag is honest **2-mode**:
`DOORD_LANDLOCK={off|enforce}`, default off (no faked `log` value Landlock cannot honor).
`DOORD_NO_SANDBOX=1` forces off (shared kill-switch with seccomp). The path allowlist
ships as an **empirically-refined seed** (the supervisor's known filesystem touch-points
+ the Tier-3 genny `openat` audit data), then is **tuned on genny**: boot
`DOORD_LANDLOCK=enforce`, run login → logout → greeter-recycle → re-login, widen the set
off the `EACCES` journal trail (kernel-audit denial records where available) until the
cycle is clean. A miss is a *recoverable* login failure — `EACCES` in the journal + the
kill-switch — not a silent lockout. Only after the genny cycle is clean does the shipped
`dist/systemd/doord.service` gain `Environment=DOORD_LANDLOCK=enforce` (mirroring the
`DOORD_SECCOMP`/`DOORD_SPAWNER` retention pattern — reversible while Tier 4 proves out).

**(part 4 — ABI floor V1).** The handled access set is pinned to **`ABI::V1`**
(Execute/Read/Write/Dir/Make/Remove). Device-file `ioctl` governance
(`LANDLOCK_ACCESS_FS_IOCTL_DEV`) only enters the handled set at ABI V5; declaring it on
genny's newer kernel would govern the supervisor's DRM-master and VT `ioctl`s and lock
out login unless `IoctlDev` were also granted on `/dev/dri` and the VT. V1 governs the
operations whose denial actually protects `/home`, `/etc/shadow`, `/var`, etc. Raising
the ABI to also confine device ioctls (granting `IoctlDev` on the device paths) is a
documented follow-on, not part of the initial seed.

## Alternatives considered

- **Kernel-audit iterate as its own mode** (enforce + 6.15 Landlock audit as a distinct
  flag value). Rejected as a *mode*; folded into the genny tuning step as a diagnostic
  technique. It is still break-then-widen (Landlock cannot log-and-allow), so it does not
  warrant a separate shipped flag value, and a `log` value that meant "enforce-but-audit"
  would be dishonest parity with seccomp.
- **Permissive dry-run probe** (fork a child that tries each access before the real
  enforce). Rejected — catches static-path misses but is blind to runtime-only paths (a
  VT switch, a re-greet, a conditional DRM re-open), so those misses still surface as a
  live lockout and the genny boot cycle is still required. Extra harness for weaker
  assurance than A already gives.
- **Declare the newest ABI (V5+) for a tighter filter out of the gate.** Rejected for the
  seed — it pulls device-ioctl governance in and risks a DRM/VT login lockout; deferred
  to a hardware-tuned follow-on.
- **Skip the flag, enforce at the shipped default immediately.** Rejected — no permissive
  stage means an un-tuned seed is a login failure on the real machine; the default-off
  flag + genny cycle is the only safe path to a correct allowlist.

## Consequences

- `landlock` added to `doord/Cargo.toml` (+ workspace dep). Resolved 0.4.5.
- `hardening.rs` gains `LandlockMode {Off, Enforce}` + `apply_landlock(mode) ->
  RulesetStatus`, called supervisor-only after `apply_seccomp`. `apply_baseline` and the
  seccomp path are unchanged. A fork-isolated confinement test proves the ruleset denies
  an out-of-allowlist path while permitting `/usr` on a Landlock-capable kernel.
- New `DOORD_LANDLOCK={off|enforce}` env flag; `DOORD_NO_SANDBOX=1` overrides to off.
- The path allowlist is a **seed**, tuned on genny (an `EACCES`/audit-driven hardware
  step, recorded like the Tier-3 log run) before the shipped unit flips to enforce.
- Threat-model update in `.agent/SECURITY/`: the supervisor's confined *path* surface,
  noting the spawner/worker/session lineage stays unconfined by design (D-0015 exclusion)
  and that the ABI-V1 floor leaves device ioctls ungoverned until the follow-on.
- Durable rule: **any new supervisor filesystem access must extend the
  `SUPERVISOR_RO_PATHS`/`SUPERVISOR_RW_PATHS` seed and be re-validated on genny** — a new
  path the supervisor opens without a matching rule is a login-time `EACCES`.
- Closes M5's hardening arc (Tier 0 baseline → Tier 1 unit → Tier 2 spawner → Tier 3
  seccomp → Tier 4 Landlock), leaving Tier 5 (PAM residual) as the documented exclusion.

## Ratification note

Critical (privilege boundary / login path), so pre-ratified before the code is committed
per the lifecycle rule. The inert shared substrate (crate + flag-gated, default-off
`apply_landlock`) was built ahead of ratification because it is identical across every
rollout option and changes nothing at runtime until the flag is set; this decision fixes
the library, the enforce-only rollout, and the ABI floor before that substrate is
committed or the shipped unit is touched.
