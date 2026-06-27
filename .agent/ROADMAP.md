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
- [x] **handoff / re-greet orchestration — D-0008** (built; not yet validated live):
      doord owns the greeter lifecycle. New greeter worker (`worker::run_greeter`):
      passwordless **greeter-class** logind session (PAM `door-greeter`) → seat
      access, then forks `cage -- door-greeter` as the greeter user on the VT.
      `ipc::serve` is now the **login loop**: greet → serve → on `Start`
      **terminate the greeter and wait (SIGTERM→SIGKILL) before** the session worker
      takes the VT (`GreeterHandle::terminate`, S14) → wait session → re-greet, with
      a crash-loop backoff. `DOORD_GREETER_{USER,PAM_SERVICE,CMD}` config. 34 tests
      green, clippy clean.
      `depends:` host-compositor + greeter surface
- [x] **systemd unit + packaging** (provisional — not validated live):
      `dist/systemd/doord.service` (daemon owns the greeter; `RuntimeDirectory=/run/doord`,
      names the greeter user, seat/VT env, `Conflicts=getty@tty1`),
      `dist/pam.d/{doord,door-greeter}`, `dist/sysusers.d/door.conf` (the `door-greeter`
      user). `door-greeter.service` was **removed** (doord owns the greeter, D-0008).
      `PKGBUILD` + `door.install` package it all, **installed disabled**, never touch
      the active DM; `door.install` prints the reversible enable + two-command TTY
      revert.
      `depends:` handoff / re-greet orchestration
- [x] **lockout hardening — live-enable postmortem (RC1–RC4, proven)** (2026-06-26;
      see `.agent/REPORTS/2026-06-26-m6-lockout-postmortem.md`). First `enable --now`
      locked the machine; postmortem found **four** root causes, all fixed and
      **proven on hardware** (commit `7aef481`): **(RC1)** `/run/doord` `0700
      root:root` unreachable by the greeter user → `ipc::bind` group-owns it
      `0750 root:<greeter>`; **(RC2)** recoverability — `Conflicts=display-manager`
      + `StartLimit*` + `ipc::serve` gives up after `GREETER_MAX_RAPID_FAILURES`
      (VT → `VT_AUTO`/`KD_TEXT`, clean exit); **(RC3)** handoff orphaned cage →
      greeter-worker SIGTERM-forwards to cage + `PR_SET_PDEATHSIG` backstop;
      **(RC4)** non-UTF-8 session locale black screen → `privdrop::sanitized_env`
      passes `LANG`/`LC_*` + `C.UTF-8` fail-safe. 38 tests green, clippy clean.
      Live boot: greeter → auth → handoff → Plasma rendered.
      `depends:` systemd unit + packaging
- [x] **reversible enable + TTY revert (validated, with RC5 follow-up)** (2026-06-26):
      enable path proven on hardware; revert (`disable --now doord` + `enable --now
      sddm`) demonstrated to restore the previous DM. Surfaced **RC5** (material):
      a clean `systemctl stop`/`disable` of doord does **not** reset a graphics-mode
      VT, so the revert can leave tty1 frozen until the fallback DM starts — `getty`
      on another VT + SSH keep it recoverable (not a true lockout). `door.install`
      revert note updated (`--now` required; known-issue documented). RC5 code fix
      queued below.
      `depends:` lockout hardening — live-enable postmortem (RC1–RC4, proven)
- [x] **live install test** (2026-06-26): installed via the package (`door 0.0.0-1`,
      `pacman -Qo /usr/bin/doord`), enabled, logged in for real through the greeter
      on the VT into Plasma; reverted to sddm with the revert in hand.
      `depends:` Arch PKGBUILD, reversible enable + TTY revert (validated)
- [x] **RC5 — reset the VT on admin teardown** (2026-06-27; material; see postmortem
      RC5): doord now installs a SIGTERM/SIGINT teardown handler (`ipc.rs`,
      `install_teardown_handler`/`handle_teardown`) that restores `VT_AUTO`/`KD_TEXT`
      **only while it holds the VT at the greeter** (`GREETING` flag), then re-raises
      with the default disposition. Extends the RC2 *crash*-path guarantee to the
      *admin stop/disable* path; a live session (no parent-death signal) keeps its VT
      and survives a doord restart. 38 tests green, clippy clean. **Not yet
      re-validated on hardware.** `depends:` reversible enable + TTY revert (validated,
      with RC5 follow-up)
- [x] **RC6 — greeter shader-cache home** (2026-06-27; cosmetic; see postmortem
      RC6): the greeter env (`worker.rs::launch_greeter`) now sets `XDG_CACHE_HOME`
      to the greeter's logind runtime dir (`XDG_RUNTIME_DIR`, 0700/writable), so
      Mesa stops logging `Failed to create //.cache` and its shader cache works.
      Proven on hardware 2026-06-27 (greeter log clean).
- [x] **RC7 — greeter death pre-handshake must not wedge doord** (2026-06-27;
      material; see postmortem RC7): the serve loop now waits on the greeter via a
      non-blocking accept + bounded poll (`accept_with_greeter_watch`,
      `GreeterHandle::reap_if_exited`); a greeter that dies before connecting is
      reaped, the VT is reset, and the failure counts toward give-up/backoff —
      instead of blocking forever on `accept()`. 38 tests green, clippy clean.
      **Not yet re-validated on hardware** (the wedge only reproduces under an
      occupied seat0; clean-boot switch is unaffected). `depends:` RC5 — reset the
      VT on admin teardown
- [x] **RC8 — session stdio paints the VT console** (2026-06-27; cosmetic; see
      postmortem RC8): `spawn.rs::take_controlling_tty` kept the VT as the session's
      controlling terminal **and stdin**, but now leaves std{out,err} on the daemon's
      inherited streams (the service journal) instead of dup2'ing them onto the VT —
      so the compositor's startup warnings (kwin/xkbcomp "multiply defined", "Lost
      connection to Wayland compositor", etc.) go to the journal, not the framebuffer.
      Proven on hardware 2026-06-27 (console clean on login). `depends:` RC5 — reset
      the VT on admin teardown
- [x] **RC9 — free the seat on teardown/re-greet** (2026-06-27; material; **proven
      on hardware**; see postmortem RC9): a door session's compositor runs under
      `user@1000` behind a self-respawning supervisor (`kwin_wayland_wrapper`) and
      outlives the logind session, doord, and `terminate-session` alike, squatting
      the seat's DRM master so the next login manager (sddm, or doord's re-greeted
      cage) can't acquire the GPU → bare blinking cursor. Fix (`ipc.rs`):
      `free_seat()` kills the **process group** of each `/dev/dri/card*` holder
      (`SIGTERM`→`SIGKILL`, taking the supervisor down so nothing respawns), wired
      to (1) seat-claim before each greet, (2) an interruptible session-wait that
      acts on a teardown flag, and (3) a panic hook. Owner decision: sessions die
      with doord (every exit incl. crash). Validated live: doord seat-claimed a
      stuck compositor and greeted; `stop doord; start sddm` reverted cleanly. 26+3
      tests green, clippy clean. `depends:` RC7 — greeter death pre-handshake

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
