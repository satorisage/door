# DECISION-0021 — DE work graduates to its own project: foyer (door stays the DM)

**Status:** Binding
**Reversibility:** cheap (moving docs + standing up a repo; nothing built yet on either side)
**Date:** 2026-07-04
**Ratified:** 2026-07-04
**Project:** door

## Context

The desktop-environment thread grew out of the D-0019 correction (KWin lacks
`ext-session-lock-v1`, so door-lock cannot lock the owner's own Plasma desktop):
first the door-compositor idea (`.agent/IDEAS/2026-07-04-door-compositor.md`,
Tier A kiosk / Tier B session compositor / Tier C full DE), then the owner-requested
anatomy map (`.agent/REPORTS/2026-07-04-desktop-environment-anatomy.md` — what runs
a Plasma session, door's module map, honest costs, the shippable sequence), then
the door-parts naming convention (D-0020: doorstep, doorframe, sill, doorbell,
threshold, latch, doorman, keyhole; umbrella name parked, candidate "foyer").

Both documents recorded the same graduation path: Tier B and above is "a new
product with its own vision, threat model, and roadmap — not a door milestone."
door's ratified scope is a display manager (login + lock surfaces); a session
compositor and shell family is a different product with door as one module.
On 2026-07-04 the owner said "we are moving the DE work" and confirmed the
sibling-project reading plus the name.

## Decision

1. **The DE work moves to its own `.agent`-tracked sibling project: `foyer`**
   (`~/Projects/foyer`). Its scope, vision, threat model, and roadmap are
   authored there (vision-first bootstrap per the dotagent flow), not inherited
   from door. foyer owns the Tier B+ modules of the anatomy map's §4:
   **doorframe** (session compositor), **sill** (panel), **doorbell**
   (notifications), **threshold** (session manager), **latch** (idle),
   and the late/optional **doorman** (polkit agent) + **keyhole** (secrets).
2. **The umbrella name is unparked: the environment is `foyer`.** This
   supersedes, in part, D-0020's clause that parked the umbrella name until a
   shell exists (the door-parts convention itself and all module name
   reservations stand unchanged and extend to foyer).
3. **door stays the DM project, unchanged in scope.** door, door-lock,
   door-theme, and door-settings remain here. **doorstep** (Tier A, the cage
   replacement in door's pre-auth TCB) remains a *door* concern and stays an
   unratified candidate in door's idea inbox — it is door's greeter host, not
   DE work, regardless of foyer's fate.
4. **The founding documents copy to foyer; door keeps its originals.** The
   anatomy report is foyer's founding brief; door's copies stay in place (with
   graduation pointers appended) as this project's historical record.

## Alternatives considered

- **Ratify the DE as door milestones in this repo.** Rejected — contradicts the
  recorded graduation analysis: the DE has its own vision/threat-model/roadmap
  axes, and door's scope, criticality rubric, and hard constraints (e.g. "No
  network, ever", pre-auth TCB discipline) are login-surface rules that do not
  transfer wholesale to a session compositor and shell. Absorbing it would
  bloat door's scope and dilute both (Principles 6/7).
- **Name the project after the load-bearing module (doorframe).** Rejected by
  the owner — the project is the environment, not just the compositor; the
  modules live inside it. Naming it foyer now costs one partial supersession
  of D-0020's parking clause.
- **Park the DE work.** Rejected — the owner is committing to it; parking was
  the status quo this decision ends.

## Consequences

- `~/Projects/foyer` is stood up as a git repo seeded with the founding brief
  (the anatomy map) and the graduated idea material; its `.agent` scope comes
  from a vision-first bootstrap run in a foyer session (owner + agent-init),
  not copied from door.
- door's idea file `2026-07-04-door-compositor.md` gets a graduation pointer:
  its Tier B fork is resolved (→ foyer); the file itself **stays in the inbox**
  because the Tier A / doorstep fork is still open and door-owned. Forks 1–2
  of its "Forks to resolve at graduation" are now answered (Tier B → its own
  repo; Tier A remains a door candidate); forks 3–4 transfer to doorstep's
  eventual door DECISION.
- The anatomy report gets the same pointer appended.
- D-0020's index row gains a partial-supersession annotation (umbrella-name
  clause only).
- Cross-project seams that now exist: door is the DM module *of* foyer while
  remaining its own product; door-lock is foyer's locker day one (doorframe
  will speak `ext-session-lock-v1`); M10's demo bound ("not on the owner's own
  stack") dissolves the day doorframe can host it. None of these seams change
  door's code today.
