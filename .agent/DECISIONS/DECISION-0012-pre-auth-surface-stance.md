# DECISION-0012 — pre-auth surface stance: caps-lock is TCB-neutral; the rest stays parked

**Status:** Binding
**Date:** 2026-06-29
**Ratified:** 2026-06-29
**Project:** door
**Relates to:** Scope Principle 1 (security dominates elegance dominates features) +
Principle 7 (threat-model-first) + the privilege-separation hard constraint;
`.agent/IDEAS/2026-06-29-graphics-capability-audit.md` (group E/F gating).

## Context

The M7 graphics push shipped the TCB-neutral A–E polish (shaders, canvas widgets,
unprivileged settings UI — nothing touching the trust boundary). The remaining
audit items are gated *on purpose* because they grow what runs or is read on the
**unauthenticated login screen**. The owner asked to surface the gating decision
and chose the conservative path: open the surface only where it costs nothing.

The gated items are not uniform — they split by threat model:
- **Caps-lock indicator** — the greeter reads its *own* keyboard-lock state; no new
  daemon protocol, no information disclosure beyond "caps is on."
- **Battery / network / keyboard-layout indicators** — local reads, but they display
  system state to a shoulder-surfer pre-auth.
- **User list + avatars** — needs the privileged daemon to *enumerate users* → a new
  pre-auth IPC message + username-enumeration disclosure.
- **F1 custom `.wgsl` background** — runs admin-supplied shader code on the login
  screen (needs naga validation + resource/time bounds).
- **F2 video / GIF wallpaper** — pulls a media-decode stack (a known CVE class) onto
  the pre-auth surface.

## Decision

**Reclassify the caps-lock indicator as TCB-neutral and build it now. Keep every
other pre-auth-expanding item parked for v1**, each requiring its own dated
threat-model + decision before it can graduate (it does not get "barreled").

- Caps-lock detection is a **purely local read** of the keyboard `capslock` LED
  (`/sys/class/leds/*::capslock/brightness`) — no daemon, no privileged path, world-
  readable, discloses nothing sensitive. `None` (no such LED, e.g. some laptops) →
  the hint is simply hidden rather than asserting a state it cannot know.
- Liveness: re-read on a CapsLock key press (instant when focused) with a 1 Hz Tick
  poll as the backstop (covers a press that lands while unfocused).

## Consequences

- `door-greeter/src/app.rs`: a `caps_lock` state field seeded from `caps_lock_on()`,
  a CapsLock key subscription → `Message::CapsLockChanged`, the Tick backstop poll,
  and a warning row ("⇪ Caps Lock is on", `error_color`) that slips in under the
  password field only while on (no empty gap otherwise).
- **Parked (unchanged):** user list + avatars, battery/network/keyboard-layout
  indicators (group E); custom `.wgsl` (F1) and video wallpaper (F2). The ROADMAP
  M7-E / M-F entries remain DECISION-gated; F2 is still a likely permanent *no* for
  v1. The pre-auth daemon↔greeter IPC surface is unchanged by this decision.
