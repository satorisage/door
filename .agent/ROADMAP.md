# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

### M3 — Minimal greeter (functional, ugly)

Toolkit ratified: **Iced + iced_layershell** (D-0006). The greeter is the
unprivileged, untrusted half: it speaks the `protocol` crate over the daemon
socket, holds no credential beyond submit, and starts no session itself.

- [x] **protocol client** (`door-greeter/src/client.rs`): connects `DOORD_SOCKET`,
      runs the `Hello`/`Welcome` handshake, typed API over the conversation
      (`list_sessions`, `begin_auth` + `recv_auth`/`reply`, `start`, `power`).
      Blocking I/O isolated; 4 unit tests over a scripted socket pair.
      `depends:` M1
- [x] **Iced layer-shell shell** (`door-greeter/src/app.rs`): `wlr-layer-shell`
      overlay via `iced_layershell` (Overlay layer, all-edge anchor, Exclusive
      keyboard); Iced app (State/update/view) with the client on a background
      worker thread, bridged via an `iced_futures::stream::channel` subscription
      that hands the UI its command channel through `Message::WorkerReady`.
      `depends:` D-0006
- [x] **picker + auth UI**: session pick_list, username + password fields, Sign-in
      button; renders each prompt, sends the `AuthReply` (auto-answers the password
      prompt if pre-typed), then issues `Start` on success. Password cleared on
      submit. *(In-memory plaintext during entry is inherent to the text field;
      deeper zeroization is the M5 audit.)*
      `depends:` protocol client, Iced layer-shell shell
- [x] **power controls (greeter side)**: suspend / reboot / power-off buttons →
      `Request::Power`. **Daemon-side Power still returns "not yet available"**
      (`ipc::dispatch`); the greeter surfaces that as a status line. Implementing
      logind Power in the daemon is a small follow-up (deferred, not blocking M3).
      `depends:` Iced layer-shell shell, protocol client
- [ ] **live end-to-end**: drive auth → `Start` against a live `doord` under a
      layer-shell compositor.
      *(2026-06-26: greeter **verified running** against a wlroots compositor —
      connects, handshakes, lists sessions, renders, holds the connection, no
      crash. A `DOORD_GREETER_DEV=1` mode (floating + on-demand keyboard) added for
      safe nested smoke-testing without keyboard lockout. Remaining: a human
      driving the full auth+start. **Note:** `cage` (this build) lacks
      `wlr-layer-shell`, so the production host compositor is an open item —
      sway/weston/labwc, or reconsider plain-iced toplevel; deployment/M6 detail.)*
      `depends:` picker + auth UI, power controls (greeter side)

**Done-when (M3):** the greeter, an unprivileged Wayland layer-shell client, lists
the daemon's sessions, drives the PAM conversation to a successful auth, and starts
the chosen session against a live `doord` — holding no credential beyond submit and
never touching privilege. Ugly is fine; M4 makes it beautiful.

> **Carry-forward (D-0003 H5 text vs code):** H5 reads `setresgid → initgroups →
> setresuid`; the shipped+reviewed `privdrop` does `initgroups → setresgid →
> setresuid` (both safe: groups+gid before uid, post-drop verify, refuse uid 0).
> A doc-only correction to H5's text; not blocking. (Also tracked in STATE §5.)

## Backlog (future milestones, not yet sequenced)

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

- ~~Decide greeter toolkit (GTK4 / Qt-QML / Iced / bespoke wgpu)~~ — **resolved
  2026-06-26: Iced + iced_layershell (D-0006).**

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
