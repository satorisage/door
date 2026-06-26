# Project Scope: door

## Overview

**door** is a beautiful, security-first **display manager** (login manager) for
Linux — the program that owns the screen before any user session exists and
decides which session begins. It is a *full* login manager (it owns auth,
seat/VT, and session handoff), paired with a Wayland-native greeter that is
genuinely gorgeous. Success: door replaces greetd+ReGreet on Stephen's own
machines, is visibly more elegant than SDDM, and carries a *defensible* security
story — a small, audited, privilege-separated core — that a from-scratch login
manager normally cannot claim.

## Pairings

- **display-manager** — the core domain: pre-session greeter, the greeter
  system-user with no `$HOME`, VT/seat ownership, the session-handoff contract,
  and the lock-you-out failure mode.
- **arch-linux** — primary target (CachyOS/Arch); packaging, PAM/`logind`
  integration, boot-chain fragility, reversible service switches.
- **offensive-security** — the "SECURE" headline: threat-model-first, name what
  each milestone defends against and what it does not, attacker as a first-class
  persona.
- **ui-ux** — the "beautiful" half: the greeter as a crafted, animated,
  themeable surface, not a stock form.

## Principles, in priority order

These extend PERSONAL-PRINCIPLES.md for door. Lower number wins on conflict.

1. **Security dominates elegance dominates features.** When a pretty feature
   widens the privileged attack surface, the feature loses. The trust boundary
   is the product; the wallpaper is decoration on top of it.
2. **Privilege separation is the architecture, not a feature** (specializes
   Principle 6/7). A minimal *privileged* daemon owns PAM, `logind`, VT/seat,
   env sanitization, and session spawn. A separate *unprivileged* greeter
   renders UI and holds no authority beyond "ask the daemon to try these
   credentials." A compromised greeter must never be root. One fact, one owner:
   credentials and privilege live behind the IPC seam, never in the UI process.
3. **Vision down to detail — the handoff contract before the wallpaper**
   (Principle 1). Design how the greeter tells the daemon which session to
   start, and what tears the greeter down so handoff fires, *before* any
   theming. The prettiest greeter that botches handoff drops you to a respawn
   loop.
4. **Reversibility is a design requirement, not a nicety** (Principle 2). The
   only real test is the destructive switch, which can lock you out. Every
   change keeps the previous DM installed-but-disabled, and the revert is two
   TTY commands the installer prints. A rollback you reconstruct from memory at
   a black screen is an unanticipated migration.
5. **The tracked config files are the engine; the installer only places them**
   (Principle 5). door's behavior is declarative config deployed to known
   paths; `setup-*.sh` only installs files + assets system-wide, sets group
   membership, and reversibly flips the unit. No behavior smuggled into the
   installer.
6. **Honest bounds — name what the greeter cannot do** (Principle 9). No live
   preview; pre-session limits (no user `$HOME`, no day/night auto-switch); the
   security claims attach to PAM success, the elegance claims to the form.
   State which were *observed* vs. *derived* (Principle 13).
7. **Threat-model-first.** Every milestone names its threat model: what it
   defends against (shoulder-surf, evil-maid, greeter compromise → root) and
   what it explicitly does not.

## Hard constraints

- **Privilege separation is non-negotiable.** The greeter runs unprivileged;
  the privileged path is minimal, drops privileges, and is the only thing that
  touches PAM/VT/seat/session-spawn. Greeter compromise ≠ root.
- **Rust for the whole stack** (D-0002) — memory-safe across the trust
  boundary; the most security-critical code does not get a use-after-free class.
- **No greetd, no greetd protocol** (D-0001). door is fully self-contained with
  a bespoke IPC protocol; it takes no runtime or wire dependency on greetd.
- **No network, ever.** No XDMCP / remote login. Local Unix-socket IPC only,
  peer-credential checked.
- **Exactly one DM owns the machine.** door ships installed-but-disabled until
  deliberately enabled; never enabled alongside another DM racing VT1.
- **Reversible from a TTY in two commands**, both printed by the installer, with
  the previous DM kept installed as fallback.
- **All greeter assets installed system-wide and world-readable** — the greeter
  user has no human `$HOME`; an asset in `~` is a guaranteed fallback-to-ugly.
- **The greeter is diagnosable, not a silent flicker loop** — its stderr goes to
  the journal; a failed greeter shows *why*, never just blinks.
- **Secrets hygiene:** credentials are zeroized after the PAM conversation,
  never logged, never written to disk, never held in the UI process longer than
  the submit.

## Capabilities currently in scope

### Privileged core (`doord`)
- PAM authentication conversation (incl. multi-prompt / 2FA-capable flows).
- `logind` (and optional `seatd`) seat/VT/session management.
- Session discovery from `/usr/share/wayland-sessions` + `/usr/share/xsessions`.
- Session spawn honoring each `.desktop` `Exec=` (incl. wrapper launchers like
  `start-hyprland`), with a sanitized environment.
- Local IPC server (Unix socket, peer-cred checked) — the only greeter↔core seam.
- Privilege drop + sandbox bounding (seccomp/landlock as feasible).

### Unprivileged greeter (`door-greeter`)
- Wayland-native UI: session picker, user selection, password field, power
  controls (reboot/poweroff/suspend via the core, never directly).
- A first *beautiful default*: animated, themeable, cursor/font/wallpaper
  system-wide.
- Speaks the IPC protocol to the core; holds no credential beyond submit.

### Tooling
- Declarative tracked config + a reversible installer that prints its revert.
- A `--check` / non-destructive preview mode (bounded — see Out of scope: no
  true live preview exists).

### Planned but not yet specified (preserve, do not extend)
- **Greeter UI toolkit.** GTK4 / Qt-QML / Iced / bespoke wgpu — a Material
  decision deferred (D-0002 fixes the language, not the rendering stack).
- **Theming engine.** Beyond the first beautiful default — preserved, not
  developed until one default ships and proves out.

## Out of scope

- **Network / remote login (XDMCP)** — excluded permanently; contradicts the
  no-network constraint. Nothing would justify adding it.
- **X11-rendered greeter** — Wayland-first; door may *launch* X11 sessions, but
  the greeter itself is a Wayland client. Adding an X greeter needs a check-in.
- **Session locking (lock screen)** — door is login-only for v1. Locking is a
  candidate (the same privilege seam could serve it) but is not committed; needs
  a check-in to bring in.
- **Multi-seat / exotic seat configs** — v1 is single-seat. Multi-seat needs a
  check-in.
- **A general theming/plugin marketplace** — out until the core + one default
  exist.
- **greetd interop / compatibility** — decided out (D-0001); door replaces that
  layer rather than joining it. Re-adding needs a superseding decision.
- **Non-systemd init support** — v1 assumes `logind`; `seatd`-only/elogind
  portability is a later candidate, not v1.

## Removal authority

Anything in the codebase that does not map to a capability in "Capabilities
currently in scope" above is a candidate for removal — default to removing it.
File a check-in only when removal is non-trivial, ambiguous, or load-bearing
(anything touching the privilege boundary is load-bearing by definition).

## Criticality rubric

**Critical** (hard-stop, do not touch related work):
- Anything that crosses or weakens the **privilege boundary** (greeter gaining
  authority, daemon trusting greeter input unchecked, IPC auth).
- The **PAM/auth flow** and **secret handling** (storage, lifetime, logging).
- The **session-handoff contract** and **VT/seat ownership** (lockout risk).
- **IPC protocol** shape and the **greetd-compat** decision.
- Implementation-**language** choice for the privileged path.
- Anything that could **lock the machine out of the GUI** without a tested revert.

**Material** (continue on parallel work, avoid downstream):
- Session discovery/launch details, environment sanitization specifics.
- Installer/packaging structure, service unit wiring.
- Greeter↔core message additions that don't touch the trust model.

**Minor** (continue freely):
- Greeter visuals, animation, theming, cursor/font/wallpaper.
- Copy, layout, non-security UX polish.
- Docs and comments.

## Default check-in mode

Hybrid per the operating manual — with the rubric above. Note: because the
failure mode is lockout, *any* change to the live DM service or handoff defaults
to hard-stop until a TTY revert is in hand, even if otherwise Material.

## Active milestone

**Milestone:** M1 — Privileged core skeleton (see ROADMAP `## Active`). M0 is
done: scope committed; D-0001 (no greetd, bespoke protocol) and D-0002 (Rust)
ratified Binding; Cargo workspace scaffolded.
**Definition of done (M1):** `doord` runs a PAM auth conversation over a
peer-cred-checked Unix socket, drops privileges, and a threat model for the auth
path is written.
**Active blockers:** none — M1 is unblocked.

## Project-specific glossary

- **door** — the whole login manager (the product).
- **doord** — the *privileged* daemon: PAM, `logind`, VT/seat, session spawn,
  IPC server. The TCB.
- **door-greeter** — the *unprivileged* UI process. "greeter" alone always means
  this, never the whole manager.
- **greeter user** — the system user `doord` runs the greeter as; has no human
  `$HOME`, only `video`/`input`/seat access.
- **session** — disambiguate every use: a **logind session** (seat/kernel
  object) vs. a **desktop session** (`.desktop` `Exec=`).
- **login** — the **auth event** (PAM success) vs. the **visible form**. Security
  claims attach to the former, elegance claims to the latter.
- **the switch** — enabling door's systemd unit as the machine's DM; the only
  real, destructive test.
