# DECISION-0020 — Module naming: the door-parts convention

**Status:** Binding
**Reversibility:** cheap (a naming convention for *future* modules; nothing
shipped is renamed)
**Date:** 2026-07-04
**Ratified:** 2026-07-04
**Project:** door
**Relates to:** the module map in
`.agent/REPORTS/2026-07-04-desktop-environment-anatomy.md`;
`.agent/IDEAS/2026-07-04-door-compositor.md` (the modules this will name);
D-0002 (one stack, one family); the existing names it generalizes
(`door-greeter`, `door-lock`).

## Context

The DE-anatomy briefing (2026-07-04) laid out a module map that could grow
door toward a desktop environment (kiosk compositor, session compositor,
shell, idle daemon, notifications, …). The owner asked for a naming
convention "where each module is named as to what it represents — door is
the front door." The observation that settled it: the shipped names already
follow such a convention unconsciously — `door-greeter` is the person at the
door, `door-lock` is a part of the door. The convention just needed to be
named and extended.

## Decision

**Every door module is named as a part of, or a figure at, the door** — the
"door-parts" family (owner-ratified 2026-07-04, option A). Concretely:

- The name states what the module *represents at the door*, not what it
  technically is: the notifications daemon is **doorbell**, not
  `door-notifyd`.
- Reference mapping (from the anatomy report; binds when each module is
  ratified, not before):
  - **doorstep** — the pre-auth kiosk compositor (greeter host, cage
    replacement; supersedes the working name "door-stage").
  - **doorframe** — the session compositor (the structure every window
    hangs in).
  - **doorbell** — the notifications daemon.
  - **doorman** — the polkit agent (checks the list before letting an
    action in).
  - **latch** — the idle-then-lock daemon (the latch clicks the lock shut
    on its own).
  - **sill** — the panel/status bar (indicators sit on it).
  - **threshold** — session start/handoff/logout (what you cross when auth
    succeeds).
  - **keyhole** (or **keyring**) — the secrets service.
  - **door-portal** — the XDG portal backend (freedesktop already speaks
    door words).
- **Binary/package names keep the `door-` prefix wherever the bare noun is
  a collision risk** (`door-latch`, `door-sill`, `door-keyhole`); the short
  name is how the module is spoken and documented. Natural compounds
  (doorbell, doorman, doorstep, doorframe) need no prefix — they carry the
  brand inside the word.
- Existing names stand: `door`, `doord`, `door-greeter`, `door-lock`,
  `door-settings`, `door-theme`. No renames; the convention governs
  arrivals.
- The umbrella name for the eventual full environment (e.g. **foyer** —
  what the front door opens into) is *deliberately not decided*; it waits
  until a shell exists (revisit at doorstep/doorframe ratification).

## Alternatives considered

- **B — house-rooms family** (porch, hearth, blueprint, dusk, safe, hall):
  warmer and wider vocabulary, but it abandons the door brand (`pacman -Ss
  door` stops finding the family), and bare common nouns (`frame`, `safe`,
  `hall`) are `$PATH`/crates.io collision bait. Rejected.
- **Technical names** (`door-notifyd`, `door-comp`, `door-idle`): safe,
  greppable, forgettable. Rejected — the product's identity is that its
  names mean something; greeter/lock set the precedent.
- **Hybrid (parts + house umbrella)**: effectively adopted-in-part — the
  umbrella question is parked, and a house word remains a candidate for it.

## Consequences

- **Durable rule: a new door module must be named as a door-part** (what it
  represents at the door), with the `door-` prefixed binary form when the
  bare noun is collision-prone. Naming is settled at each module's
  ratifying DECISION.
- The IDEA file's "door-stage" working name is superseded by **doorstep**
  at graduation; `.agent/IDEAS/2026-07-04-door-compositor.md` and the
  anatomy report carry pointer notes.
- The reference mapping above is a naming *reservation*, not a roadmap
  commitment — modules still graduate one by one through their own
  decisions.
