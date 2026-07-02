# Threat model — the supervisor Landlock sandbox (M5 Tier 4)

**Scope of this model:** the filesystem confinement doord imposes on **itself**
(the long-lived supervisor) via a Landlock LSM ruleset, per **DECISION-0017**.
It picks up where the session-spawn model (`session-spawn-threat-model.md`) and
the Tier 3 seccomp confinement leave off: seccomp narrows *which syscalls* the
supervisor may make; Landlock narrows *which paths* it may reach. Both are
supervisor-only, applied after the pre-forked spawner (`DOORD_SPAWNER=1`) has
moved session/greeter spawn off doord's sandboxed lineage — without that move a
ruleset inherited across fork+execve would cage the user's whole desktop.

**Landed 2026-07-02** (`hardening::apply_landlock`, `landlock` crate 0.4.5,
ABI V1, `DOORD_LANDLOCK={off|enforce}`). Shipped-unit flip to
`DOORD_LANDLOCK=enforce` on 2026-07-02 after a clean genny enforce boot. Closes
M5.

---

## 1. Assets

- **A1 — Root filesystem authority.** The supervisor runs as root; absent
  confinement it can read or write any path. The point of this tier is to shrink
  that reach to the handful of subtrees the login role actually needs, so a
  supervisor compromise (via the IPC seam, PAM, or a dependency) cannot pivot to
  reading `/home`, `/root`, `/var`, or writing arbitrary files.
- **A2 — Secrets at rest outside the login path.** SSH keys, browser stores,
  password databases, mail — all under `/home` and `/root`, none of which the
  login supervisor ever touches. Landlock denies the supervisor a path to them
  even with root and even after a seccomp-permitted `openat`.
- **A3 — Boot / package integrity.** `/boot`, `/opt`, `/srv` are outside the
  ruleset's write set (and its read set), so a compromised supervisor cannot
  plant a boot payload or tamper with installed trees through this process.

## 2. Trust boundary

- **TB1 — The IPC seam (shared with the auth/spawn paths).** Untrusted greeter
  input reaches the supervisor here. Landlock is the containment assumption for
  "what if that input drives the supervisor to touch a file it shouldn't": the
  kernel refuses the access regardless of the code path that attempted it.
- **TB2 — Dependency surface.** Landlock also bounds the blast radius of a bug
  in a linked crate (PAM, IPC, DRM/VT libs): a rogue file access from deep in a
  dependency is denied by the same ruleset.

## 3. The ruleset

Handled access set: **all** ABI-V1 filesystem access rights (read, write,
execute, readdir, make/remove of files and dirs). Grants are path-beneath rules:

- **Read/exec (`SUPERVISOR_RO_PATHS`):** `/usr` (session `.desktop` discovery +
  shared libs/binaries mapped in), `/etc` (NSS/`nsswitch.conf`/`passwd` lookup,
  `ld.so.cache`), `/proc` (`free_seat`'s per-process `/fd` scan), `/sys` (DRM /
  device metadata), `/run` (logind/D-Bus sockets, runtime dirs).
- **Read/write (`SUPERVISOR_RW_PATHS`):** `/run/doord` (create/bind/chmod/chown/
  unlink the IPC socket), `/dev` (`/dev/dri/card*` DRM master + `/dev/tty{N}` VT).

Everything else is denied. Most sharply: `/home`, `/root`, `/var`, `/tmp`,
`/boot`, `/opt`, `/srv`, `/mnt` are absent from both sets by design — denying
them is the tier's purpose (A2/A3).

## 4. Design decisions & their rationale (DECISION-0017)

- **ABI pinned to V1, not the newest.** V5+ governs device `ioctl`s; handling
  them would require enumerating every DRM/VT ioctl the supervisor issues or
  lose DRM master / VT control — a lockout risk with no asset gain for a path
  sandbox. V1's file-access set is the whole point; devices are governed by the
  `/dev` path grant, not ioctl rules.
- **Enforce-only, 2-mode (`off|enforce`), no log mode.** Landlock has no
  `SCMP_ACT_LOG` analogue — there is no permissive/observe stage. The seed was
  therefore tuned by an **enumerate-then-enforce** genny boot (widen off the
  EACCES/audit trail until a full login cycle runs clean), not by grepping a
  log-only run. This is the one structural difference from the Tier 3 seccomp
  rollout.
- **Best-effort compatibility.** On a kernel without Landlock (or an older ABI)
  the ruleset degrades to `NotEnforced`/`PartiallyEnforced` rather than aborting
  startup; the install logs the real enforcement level so a false-green (flag
  set, nothing enforced) is visible in the journal. The genny validate script
  (`scratch/genny-tier4-validate.sh`) fails loudly on exactly that shape.
- **Applied supervisor-only, after the spawner fork, never from a test.** The
  confinement is irreversible for the process and inherited by children, so
  applying it before session-spawn left the lineage would break every login, and
  applying it in a unit test would cage the test runner.

## 5. Recovery / kill-switch

- **`DOORD_NO_SANDBOX=1`** forces the ruleset off (also disables seccomp) —
  parsed in `LandlockMode::parse` ahead of the mode value.
- **Unset `DOORD_LANDLOCK`** (or `=off`) — no ruleset, keeps seccomp.
- A too-tight seed surfaces as an `EACCES` login failure from a TTY, not a
  wedged supervisor; delete the drop-in or set the kill-switch, then
  `systemctl daemon-reload && systemctl restart doord`.

## 6. Residual risks / out of scope

- **N1 — Seed is empirically-refined, not proven-minimal.** The RO/RW sets are
  the smallest that passed a clean genny cycle, not a formally-minimal set. A
  future login-relevant path (a new NSS module dir, a distro that moves a
  runtime dir) could need a widening — the failure mode is a visible EACCES, not
  a silent bypass.
- **N2 — `/dev` and `/run` are granted whole subtrees, not leaf paths.** The
  in-code note flags tightening `/dev` to `/dev/dri` + `/dev/tty*` specifically;
  deferred as a hardening idea (broad grant validated clean, narrowing risks
  breaking a working config). Tracked in the ROADMAP backlog.
- **N3 — Landlock governs paths, not their contents or the network.** It does
  not replace the seccomp filter (syscall surface), the capability drop, or the
  "no network, ever" posture — it composes with them as the filesystem axis.
- **N4 — The user session is deliberately un-confined by this ruleset.** That is
  the whole reason for the spawner (Tier 2): the desktop must keep full
  filesystem reach. This tier hardens the supervisor, not the session.
