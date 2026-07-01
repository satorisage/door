# Scope — seccomp + Landlock sandboxing of doord (M5)

**Status:** pre-decision scope (the Tier-2 refactor needs a `DECISION` before build).
**Goal:** bound the blast radius of a doord compromise — a bug in the
pre-auth-reachable code (IPC parse, PAM relay, seat management) should not give an
attacker the full syscall/filesystem power of root. seccomp caps *what syscalls*
doord can make; Landlock caps *what paths* it can touch.

---

## The central constraint (this dictates the whole design)

seccomp filters and Landlock rulesets are **inherited across `fork` and preserved
across `execve`** — by design. doord's process lineage is:

```
main daemon ──fork(per login)──▶ worker ──execve──▶ the user's session/compositor
   (root, long-lived)              (PAM + privdrop)      (needs the FULL profile)
```

Confirmed: `hardening::apply_baseline()` runs at startup (`main.rs:51`); the worker
is forked lazily per-login (`ipc.rs:879`, inside the accept loop); the session runs
in doord's own mount namespace (no `unshare`). **So anything doord restricts on
itself flows straight into the user's desktop session** — which must run with an
unconstrained syscall set and full filesystem access.

**Corollary:** doord cannot simply self-sandbox. A tight filter applied at startup
would break every login. The sandbox must live on a process that is *not* on the
path to `execve`-ing the session.

Two hard sub-problems fall out of this:

1. **The session-exec boundary.** The thing that execs the session must be
   *unsandboxed* (or minimally sandboxed to a full-desktop profile, which is no
   protection). So the sandbox and the session-spawner must be *different
   processes*, and the spawner must be forked *before* the sandbox is applied.
2. **The PAM boundary.** PAM `dlopen`s arbitrary modules (`pam_unix` reads
   `/etc/shadow`; `pam_systemd` talks D-Bus; `pam_faillock` writes state; LDAP/SSS
   modules open **network** sockets and read `/etc/ldap`, `/var/lib/sss`, …). A
   tight seccomp/Landlock over the PAM conversation would break real-world auth
   configs we cannot enumerate. The PAM phase is effectively un-sandboxable in the
   general case.

---

## Layered plan (tractable now → needs refactor → residual)

### Tier 0 — done
`NO_NEW_PRIVS` + `PR_SET_DUMPABLE(0)` (`hardening.rs`). Cheap, already shipped.

### Tier 1 — systemd unit hardening (config-only) — DONE (2026-06-30)
**Correction to the earlier draft:** the session-inheritance constraint is *worse*
than "avoid the mount directives." Because the session is a fork+execve child of
doord, the **seccomp-based** unit directives leak into the desktop too — not just
the mount-based ones. So the genuinely session-safe surface is narrow, and the
strongest wins are **all deferred to Tier 2**:
- `RestrictAddressFamilies=AF_UNIX` — I earlier flagged this only for PAM; it is
  worse than that: the **user's session inherits it and loses AF_INET → no network
  in the desktop.** Deferred to Tier 2, not shippable at the unit level.
- `SystemCallFilter=` (Tier 3), `SystemCallArchitectures=native` (kills 32-bit
  session apps), `MemoryDenyWriteExecute=` (breaks session JIT), `RestrictRealtime=`
  (breaks session pro-audio), `RestrictNamespaces=` (breaks flatpak/browser userns),
  `ProtectHome=`/`ProtectSystem=strict`/`PrivateTmp=` (mount-ns leak),
  `ProtectProc=`/`ProcSubset=` (blinds free_seat's /proc scan) — **all deferred**.

**Shipped (session-safe, cannot break a login):** `NoNewPrivileges=yes`,
`ProtectClock=yes`, `ProtectKernelModules=yes`, `ProtectKernelLogs=yes`,
`ProtectHostname=yes` (doord and a desktop both never do these), and
`CapabilityBoundingSet=~ …` dropping only provably-unused caps
(`CAP_SYS_MODULE CAP_SYS_TIME CAP_SYS_BOOT CAP_WAKE_ALARM CAP_MAC_ADMIN
CAP_MAC_OVERRIDE CAP_LEASE CAP_SYS_PACCT CAP_BLOCK_SUSPEND CAP_NET_BIND_SERVICE`)
via the `~` *drop* form (not an allow-list) so an unenumerated needed cap is never
stripped. Verified with `systemd-analyze verify` (exit 0). The unit carries a large
comment block documenting the deferred directives + the constraint, so a future
maintainer does not naively add `ProtectHome=` and brick every login.

**Related caveat surfaced (not a Tier-1 change):** `apply_baseline` sets
`PR_SET_NO_NEW_PRIVS`, which the session inherits — so **setuid escalation inside a
door session (a terminal `sudo`, `pkexec`, `su`) does not elevate.** D-Bus/polkit
GUI actions still work (udisks2, NetworkManager, etc. are separate root services);
only the direct-setuid path is blocked. This is intentional per the code comment
but is a real compat caveat worth documenting for users, and a policy the owner may
want to revisit (it's a decision, not a bug).

### Tier 2 — the enabling refactor (architectural; needs a DECISION)
Introduce a **pre-forked, unsandboxed spawner** so the sandbox and the
session-exec live in different processes:
- At startup, *before* any sandbox, fork a minimal long-lived **spawner** helper
  that owns session launching (fork → privdrop → `execve`) and PAM (the worker role
  folds into it, or stays a child of it).
- The **supervisor** (main daemon: accept, peercred, frame I/O, seat/VT/DRM, free_seat)
  then applies seccomp + Landlock **to itself only**, and asks the spawner over a
  socket to run a session.
- The session, spawned by the un-sandboxed spawner, runs free. Ideally the spawner
  also `unshare(CLONE_NEWNS)`s per session so Tier-1 mount directives can be added
  without touching the desktop.

This is a real change to the process model (today the worker is re-exec'd
per-login; the new spawner is forked once at startup). It supersedes/extends the
worker split. **Gate with a `DECISION`** — it touches the privilege architecture
that the session-spawn threat model rests on.

### Tier 3 — seccomp allowlist on the supervisor (after Tier 2)
- Enumerate the supervisor's real syscall set empirically (`SCMP_ACT_LOG` /
  `strace` across several session types + power/suspend + re-greet + teardown).
- Allowlist: `accept4`, `recvmsg`/`sendmsg`, `getsockopt` (SO_PEERCRED), `read`/
  `write`/`close`, `openat`, `readlinkat`, `getdents64`, `kill`, `ioctl`
  **arg-filtered** to the KD/VT/DRM cmd numbers (seccomp-bpf can match the `ioctl`
  request arg), `clone`/fork, `rt_sigaction`/`rt_sigreturn`, `prctl`, `wait4`, etc.
- Roll out **fail-open first**: `SCMP_ACT_LOG` (log the syscall, allow) in
  production for a release to catch misses via the journal, *then* switch to
  `SCMP_ACT_ERRNO`/`KILL`. A missed syscall under `KILL` = the daemon dies =
  no login = machine lockout, so the log phase is mandatory.
- Keep a **kill-switch** (`DOORD_NO_SANDBOX=1`) for recovery.
- Use `libseccomp` (or `seccompiler`) — evaluate adding the dep vs hand-rolled BPF.

### Tier 4 — Landlock path allowlist on the supervisor (after Tier 2)
- Landlock ABI 3+ (kernel ≥ 5.19): restrict the supervisor to read/write/exec on
  exactly: `/proc` (free_seat scan), `/dev/dri/card*` (DRM), `/dev/tty*` (VT),
  `/run/door` (socket), `/usr/share` + `/usr/local/share` (session discovery, ro),
  `/etc/door` + `/usr/share/door` (config, ro). Degrade gracefully on an older
  kernel (Landlock is best-effort, like the rest of `apply_baseline`).

### Tier 5 — the PAM residual (documented, not fixed)
The PAM conversation stays **unsandboxed** (or only loosely): module diversity
makes a tight filter break real deployments. Accepted because PAM runs in the
separate spawner/worker process, is short-lived, dies with the connection, and its
inputs are already gated (peercred'd greeter, `MAX_FRAME_BYTES`, the Start gate).
Record as an explicit exclusion (honest-bounds).

---

## Sequencing, effort, risk

| Tier | Effort | Risk | Prereq |
|---|---|---|---|
| 1 systemd unit | S | low (but test every directive vs a live login) | — |
| 2 spawner refactor | L | med — architectural | **DECISION** |
| 3 seccomp | M | high (miss = lockout → mandatory log-phase + kill-switch) | T2 |
| 4 Landlock | M | med (best-effort, degrades) | T2 |
| 5 PAM residual | — | — (documented exclusion) | — |

**Testing:** a CI/integration test that launches a real session under each tier
and asserts it comes up; empirical syscall/path enumeration before enforcing; the
existing `give_up`/VT-restore recovery is the backstop if a filter wedges logins.

**Recommendation:** ship **Tier 1** now (config-only, immediate "no network"
enforcement + cap drop), and **ratify Tier 2** as its own DECISION before Tier
3/4 — the pre-forked-spawner refactor is the linchpin that makes real
kernel-level sandboxing possible without breaking the user's session. Tier 5 is an
honest, documented limit.
