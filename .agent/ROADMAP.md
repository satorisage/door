# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

*(No active milestone — M6 shipped 2026-06-27. Promote the next from `## Backlog`:
M4 (beautiful greeter), then M5 (hardening).)*

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

- **M6 — Packaging + reversible install** (2026-06-27): door ships as an Arch
  package (`PKGBUILD` + `door.install`) **installed disabled by default** — never
  enables a unit, never touches the active DM; installs `doord.service` +
  `dist/pam.d/{doord,door-greeter}` + `dist/sysusers.d/door.conf`. doord owns the
  greeter lifecycle (D-0008): greet → serve → handoff (terminate the greeter, free
  the VT) → wait → re-greet, with crash-loop backoff. Sessions are tied to doord's
  lifetime; the seat is freed by killing the compositor's process group (D-0009).
  **Done-when met, proven on hardware:** package install → `enable --now` → real
  greeter login into Plasma → two-command TTY revert (`disable --now doord` +
  `enable --now sddm`) restores the previous DM cleanly. The live-enable lockout
  postmortem found and fixed **nine** root causes, all proven on hardware — RC1
  socket-dir perms, RC2 recoverability/give-up, RC3 handoff orphan, RC4 non-UTF-8
  locale, RC5 admin-teardown VT reset, RC6 greeter shader-cache, RC7 pre-handshake
  wedge, RC8 console stdio, RC9 seat-squat-on-revert (the one that made the revert
  actually work): see `.agent/REPORTS/2026-06-26-m6-lockout-postmortem.md` and
  D-0007/8/9. 26 unit + 3 integration tests green, clippy clean.
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
