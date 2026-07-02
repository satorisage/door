# CHECK-IN — Tier 3 seccomp: two open decisions before build

**Opened:** 2026-07-01 · **Status:** awaiting owner · **Blocks:** M5 Tier 3 increment 2 build

## Context
Tier 3 increment 1 (greeter routed through the spawner + concurrent reaper, HEAD
`86739f6`) is hardware-validated on `genny` — the supervisor is now sandbox-able
without confining the desktop session, because the greeter compositor no longer
forks off the supervisor. Next is **increment 2: a supervisor seccomp filter in
`SCMP_ACT_LOG` mode** (log-before-enforce per D-0015).

No seccomp code exists yet: `hardening.rs` is Tier 0 only (`NO_NEW_PRIVS` +
`PR_SET_DUMPABLE`), no seccomp crate in `doord/Cargo.toml`, and `spawner.rs`/`ipc.rs`
only reference the future filter in comments. The filter must be applied **after
`fork_spawner`** — the spawner child stays unsandboxed since it `execve`s sessions
(the whole reason increment 1 landed first).

## Open decision 1 — seccomp library
- **seccompiler** (Rust-native, Firecracker) — no C dep, cleaner supply chain. *(my recommendation)*
- **seccomp** (libseccomp FFI) — battle-tested, adds a C system-lib build dep on genny.
- **raw libc prctl/BPF** — zero new deps, own all arch/syscall-number tedium.

## Open decision 2 — decision-first vs code-first
Security-surface + new-dependency work → per the manual this wants a
`DECISION-NNNN` (allowlist rationale, log-before-enforce, post-fork placement)
before build. Recommend writing the DECISION first, then building 2a behind a flag
(e.g. `DOORD_SECCOMP=log`).

## Increment plan once resolved
1. **2a** — add crate, write supervisor allowlist, apply after `fork_spawner`, default action `LOG`.
2. **2b** — boot genny, exercise login/logout, `journalctl | grep SECCOMP`, widen allowlist until audit-clean.
3. **3** — flip default action to enforce (`ERRNO`/`KILL`) once the log run is quiet. Then Tier 4 (Landlock).

---

## RESOLVED (2026-07-01)
Both open decisions answered by **DECISION-0016**: library = `seccompiler`; decision-first
then build. Next work = increment 2a (log-mode filter). Archived.
