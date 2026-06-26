# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

### M1 — Privileged core skeleton (`doord`)
- [x] Cargo workspace scaffolded (`doord` / `door-greeter` / `protocol`)
- [x] `protocol` crate: bespoke greeter↔core message types (D-0001) + version
      handshake, redacted `Secret`, strict-in serde, shared framing (D-0003)
- [x] IPC server: Unix socket, peer-credential checked (D-0003: pathname socket,
      `SO_PEERCRED` uid gate, single-conn, length-prefixed framing, read timeout;
      E2E-tested in `doord/tests/ipc_smoke.rs`)
- [x] PAM auth conversation (multi-prompt capable) — `Authenticator`/`AuthChannel`
      seam, real PAM via `pam-client`, min-failure-delay; in-process auth-flow
      tests (`doord/src/ipc.rs`). Live root run pending (see below).
- [x] privilege drop + sanitized session environment (D-0003 H5 ordering) —
      `privdrop::{drop_to,sanitized_env}`, ordering + verify + uid-0 refusal;
      env allowlist unit-tested. `drop_to` wired to spawn in M2.
- [x] threat model written for the auth path — `.agent/SECURITY/auth-path-threat-model.md`
      `depends:` D-0001, D-0002, D-0003

**M1 status:** code-complete and unit/integration-tested. One step remains for
full definition-of-done: a **live root run** proving the real PAM conversation
and the `SO_PEERCRED` rejection of a foreign uid (cannot be done unprivileged in
this environment). Harness: `doord/examples/login_probe.rs` + `/tmp/doord-live-daemon.sh`.

Privilege drop is intentionally **out of M1's live scope**: `privdrop::drop_to`
is staged for the M2 spawn path (the daemon must stay root to keep serving PAM),
so `serve()` never drops. Its mechanism is unit-tested; its live demonstration
moves to M2's DoD (below).

**Done-when:** `doord` runs a PAM auth conversation over a peer-cred-checked
Unix socket (proven by a live root run), and the auth-path threat model is
written.

## Backlog (future milestones, not yet sequenced)

### M2 — Session discovery + launch
- discover `/usr/share/wayland-sessions` + `xsessions`
- spawn honoring `.desktop` `Exec=` (incl. wrapper launchers like `start-hyprland`)
- `logind` seat/VT/session wiring
- wire `privdrop::drop_to` into the spawn path (D-0003 H5 ordering) and
  **demonstrate the privilege drop on real hardware** — carried over from M1,
  where `serve()` stays root by design
  `depends:` M1
  **Done-when (drop):** a spawned session runs as the authenticated user's
  uid/gid with a sanitized environment, verified live (e.g. spawned `id`).

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

- **M0 — Vision, scope, architecture decisions** (2026-06-25): brief locked,
  PROJECT-SCOPE committed, D-0001 (no greetd / bespoke protocol) and D-0002
  (Rust) ratified Binding.
