# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

### M2 — Session discovery + launch
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
- [ ] `logind` seat/VT/session wiring
- [ ] **demonstrate the privilege drop on real hardware** — carried over from M1;
      run `sudo bash /tmp/door-spawn-demo.sh`, log in as the user, `Start` the
      `iddemo` session, and observe the spawned `id` report the user's uid/gid
      with the sanitized env (drop + env sanitization, live).
  `depends:` M1
  **Done-when (drop):** a spawned session runs as the authenticated user's
  uid/gid with a sanitized environment, verified live (e.g. spawned `id`).

> **Note (D-0003 H5 text vs code):** H5 reads `setresgid → initgroups →
> setresuid`; the shipped+reviewed `privdrop` does `initgroups → setresgid →
> setresuid` (the idiomatic order — `initgroups` then `setgid`, both before
> `setuid`). Both are safe (groups+gid before uid, post-drop verify, refuse uid
> 0). Drift is in the decision *text*, not the security property — flagged for a
> doc correction to H5; not blocking.

**Done-when (M2):** the daemon discovers installed sessions, and on a successful
auth spawns the chosen session as the authenticated user (privileges dropped,
environment sanitized) wired into the seat/VT via logind.

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
