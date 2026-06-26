# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

### M2 — Session discovery + launch ✓ COMPLETE (2026-06-26)
> All tasks done and live-confirmed. Summarized in `## Shipped`. Next milestone
> not yet promoted — M3 (greeter) is gated on the greeter-toolkit decision in
> `## Loose`.
- [x] discover `/usr/share/wayland-sessions` + `xsessions` — `sessions` module:
      scans data-dir roots (`DOORD_SESSION_DIRS`-overridable), parses `.desktop`
      (Name/Comment/Exec→argv, skips Hidden/NoDisplay, dedups by id), keeps `Exec`
      daemon-side; `ListSessions` returns the wire projection. Unit-tested +
      E2E in `ipc_smoke.rs`.
- [x] spawn honoring `.desktop` `Exec=` (incl. wrapper launchers like
      `start-hyprland`): `spawn::build_command` runs `exec[0]` (bare names and
      wrappers resolved against the sanitized `PATH`, absolute paths as-is) with
      the discovered argv, env cleared and rebuilt from the allowlist, cwd = home.
      Unit-tested.
- [x] wire `privdrop::drop_to` into the spawn path: the `spawn` module forks via
      `Command`, runs `drop_to` in `pre_exec` (groups→gid→uid, verified, refuses
      uid 0 — the reviewed mechanism, used as-is per the task), then execs. `Start`
      is **auth-gated** (per-connection state; refused before a PAM success) and
      **bound to the authenticated user** (the greeter's `Start` carries only a
      session id, never an identity). Protocol → v2 (`Response::Started`, an
      additive variant per D-0003 E2). Unit + IPC tests green.
- [x] **demonstrate the privilege drop on real hardware** (2026-06-25) — live
      root run passed: a `Start` after a real PAM success spawned the session and
      the daemon's child reported `uid=1000(stephen) gid=1000(stephen)` with the
      user's supplementary groups (not root's), `PATH` = the sanitized allowlist,
      and `LD_PRELOAD` unset — the privilege drop (S3/S4) and env sanitization
      (S5) confirmed end to end; child reaped (`exit status: 0`).
- [x] `logind` seat/VT/session wiring — **complete, live-confirmed 2026-06-26**
  `depends:` M1
  Implements D-0004 (pam_systemd) + D-0005 (per-login worker = logind leader). The
  daemon re-execs itself as a short-lived session worker (`worker.rs`) that owns
  the whole PAM transaction, is the logind leader, `setsid`s + takes the seat's VT
  as controlling tty before the privilege drop (`spawn::session_setup`), runs the
  session in the sanitized allowlist ∪ PAM env, then closes the session and exits.
  The daemon never enters a session scope; greeter framing stays solely in the
  daemon (worker never reads a greeter byte). Threat model S9–S13.
  `cargo test` green (30). **Multi-login live run passed (2026-06-26):** two
  back-to-back logins registered sessions 17 then 18 (leader = the per-login
  worker, each in its own `session-N.scope`), ran as `uid=1000` on `/dev/tty4`
  with `XDG_SESSION_*`/`XDG_RUNTIME_DIR`, **each closed cleanly on exit**
  (`loginctl` empty on tty4 after), and the daemon stayed in
  `system.slice/doord-m2.service` — never a session scope.

> **Note (D-0003 H5 text vs code):** H5 reads `setresgid → initgroups →
> setresuid`; the shipped+reviewed `privdrop` does `initgroups → setresgid →
> setresuid` (the idiomatic order — `initgroups` then `setgid`, both before
> `setuid`). Both are safe (groups+gid before uid, post-drop verify, refuse uid
> 0). Drift is in the decision *text*, not the security property — flagged for a
> doc correction to H5; not blocking.

**Done-when (M2): ✓ met (live, 2026-06-26).** The daemon discovers installed
sessions, and on a successful auth spawns the chosen session as the authenticated
user (privileges dropped, environment sanitized) wired into the seat/VT via
logind — demonstrated end to end with two clean back-to-back logins.

## Backlog (future milestones, not yet sequenced)

### M3 — Minimal greeter (functional, ugly)
- Wayland client: session picker, password field, power controls
- speaks the IPC protocol; holds no credential beyond submit
  `depends:` M1

### M4 — The beautiful greeter
- first beautiful default: animation, theming, system-wide assets
  `depends:` M3

### M5 — Hardening pass
- seccomp/landlock bounding, privilege-drop audit, IPC/PAM fuzzing
- secrets-zeroization audit; external review of the TCB
  `depends:` M1, M2

### M6 — Packaging + reversible install
- Arch PKGBUILD; installed-but-disabled by default
- installer prints the two-command TTY revert; previous DM kept as fallback
  `depends:` M2, M4

## Loose

- Decide greeter toolkit (GTK4 / Qt-QML / Iced / bespoke wgpu) — Material.

## Shipped

- **M2 — Session discovery + launch** (2026-06-26): `.desktop` session discovery
  (`sessions`), auth-gated identity-bound `Start`, and the full logind handoff —
  pam_systemd registration (D-0004) with a **per-login session worker as the
  logind leader** (D-0005): the daemon re-execs a short-lived worker that owns the
  PAM transaction, `setsid`s + takes the seat's VT as controlling tty, drops
  privilege, runs the session in the sanitized allowlist ∪ PAM env, then closes the
  session and exits — the daemon never enters a session scope, and greeter framing
  never reaches the worker (`O_CLOEXEC`). Threat model `SECURITY/session-spawn-threat-model.md`
  (S1–S13). **Live-confirmed** (multi-login, clean close, daemon out of scope).
- **M1 — Privileged core skeleton (`doord`)** (2026-06-25): Cargo workspace +
  hardened `protocol` crate (D-0001 message types, version handshake, redacted
  `Secret`, strict serde, shared framing, D-0003); peercred-checked Unix-socket
  IPC server; real PAM auth conversation (`Authenticator`/`AuthChannel` seam,
  min-failure-delay); `privdrop::{drop_to,sanitized_env}` (mechanism unit-tested,
  wired into the spawn path in M2); auth-path threat model. **Live root run
  confirmed:** `✓ AUTH SUCCESS` over the peercred socket as the authorized greeter
  uid, foreign uid (root) refused at the peercred gate — both the `0660`/group FS
  reach and the `SO_PEERCRED` uid match demonstrated on real hardware.
- **M0 — Vision, scope, architecture decisions** (2026-06-25): brief locked,
  PROJECT-SCOPE committed, D-0001 (no greetd / bespoke protocol) and D-0002
  (Rust) ratified Binding.
