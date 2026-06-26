# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

### M2 — Session discovery + launch
- [ ] discover `/usr/share/wayland-sessions` + `xsessions`
- [ ] spawn honoring `.desktop` `Exec=` (incl. wrapper launchers like `start-hyprland`)
- [ ] `logind` seat/VT/session wiring
- [ ] wire `privdrop::drop_to` into the spawn path (D-0003 H5 ordering) and
      **demonstrate the privilege drop on real hardware** — carried over from M1,
      where `serve()` stays root by design
  `depends:` M1
  **Done-when (drop):** a spawned session runs as the authenticated user's
  uid/gid with a sanitized environment, verified live (e.g. spawned `id`).

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
