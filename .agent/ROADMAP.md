# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

### M6 — Packaging + reversible install

**Criticality: Critical** — install/enable is the lockout-risk domain (SCOPE
rubric: "anything that could lock the machine out of the GUI without a tested
revert", and any change to the live DM hard-stops). **Revert-first:** door
installs *disabled by default*, never clobbers the existing DM, and ships a
two-command TTY revert that is **tested before** door is ever enabled.

*(Taken before M4/M5: ROADMAP `depends:` was M2, M4, but installing door as a real
reversible DM on a VT is what closes M3's production path and lets the whole thing
be exercised end to end. M4 beauty + M5 hardening follow.)*

- [x] **host-compositor + greeter surface** — *gating decision* **resolved: D-0007**
      (cage + plain-`iced` fullscreen toplevel; smallest pre-auth surface wins on
      both security and perf; layer-shell unused in v1). Greeter **reworked**:
      `iced_layershell` dropped, `run()` → `iced::application(...).window(fullscreen)`,
      `to_layer_message` removed; State/update/view/worker unchanged. Builds clean,
      4 tests green, renders as a windowed toplevel in dev mode (`DOORD_GREETER_DEV`).
      `depends:` M3
- [x] **systemd units** (written; **provisional — not validated live**):
      `dist/systemd/doord.service` (daemon, `RuntimeDirectory=/run/doord`, names the
      greeter user, seat/VT env) + `dist/systemd/door-greeter.service` (runs
      `cage -- door-greeter` as the greeter user on tty1, `PAMName=door-greeter` for
      a logind session → seat access, `Conflicts=getty@tty1`). Supporting:
      `dist/pam.d/door-greeter` (passwordless greeter session), `dist/sysusers.d/door.conf`
      (the `door-greeter` user). Two enabling code changes landed: `DOORD_GREETER_USER`
      name resolution (`config.rs`) and **greeter exits on `SessionStarted`** (steps
      aside so cage frees the VT). **Open: the greeter↔session VT handoff / re-greet
      loop** (deferred N1/N2) — the units note it; it gates the live install.
      `depends:` host-compositor + greeter surface
- [x] **Arch PKGBUILD**: `PKGBUILD` + `door.install` — packages the binaries, both
      PAM files, both units, and the greeter user; **installed disabled** (enables
      nothing, never touches the active DM). `door.install` prints the reversible
      enable + the two-command TTY revert.
      `depends:` systemd units
- [ ] **reversible enable + TTY revert (validated)**: the enable/revert *commands*
      ship in `door.install` (keep previous DM as fallback). Remaining: **test the
      revert** and settle the greeter↔session handoff so a real login + logout
      cycle works. Critical — revert proven before door is ever enabled.
      `depends:` systemd units, the handoff/re-greet lifecycle
- [ ] **live install test**: `makepkg -si`, run the enable step, log in for real
      through the greeter on the VT — tested revert in hand.
      `depends:` Arch PKGBUILD, reversible enable + TTY revert (validated)

**Done-when (M6):** door installs from a PKGBUILD (disabled by default), can be
enabled to become the machine's login manager with the previous DM kept as
fallback, a real login through the greeter starts a session, and a tested
two-command TTY revert restores the previous DM.

## Backlog (future milestones, not yet sequenced)

### M4 — The beautiful greeter
- first beautiful default: animation, theming, system-wide assets
  `depends:` M3

### M5 — Hardening pass
- seccomp/landlock bounding, privilege-drop audit, IPC/PAM fuzzing
- secrets-zeroization audit; external review of the TCB
  `depends:` M1, M2

## Loose

- ~~Decide greeter toolkit (GTK4 / Qt-QML / Iced / bespoke wgpu)~~ — **resolved
  2026-06-26: Iced + iced_layershell (D-0006).**

## Shipped

- **M3 — Minimal greeter (functional)** (2026-06-26): `door-greeter`, the
  unprivileged Iced + iced_layershell client (D-0006). `client.rs` (protocol
  client, 4 tests) + `app.rs` (layer-shell overlay: session picker,
  username/password, sign-in, power; background worker owns the blocking client,
  bridged to Iced via a `stream::channel` subscription). Holds no credential
  beyond submit; starts no session itself. **Verified live** against a wlroots
  compositor (connect → list → render → auth → `Start`); `DOORD_GREETER_DEV=1` mode
  for safe nested testing. Open follow-ups carried into M6/later: production host
  compositor (cage lacks layer-shell), daemon-side `Power`.
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
