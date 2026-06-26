# Project State

**Last updated:** 2026-06-25
**Active focus:** M2 (session discovery + launch). **M1 is COMPLETE** — live root
run on 2026-06-25 confirmed `✓ AUTH SUCCESS` over the peercred socket plus
foreign-uid rejection. M2 builds on the served socket: discover sessions, then
spawn the chosen one as the authenticated user — this is where `privdrop::drop_to`
finally fires (its live privilege-drop demo is M2's DoD).

---

## 1. Authority surface — where to look for X

Single map of canonical tracking surfaces. **Start here** when you don't
know which file to open. List every tracked file or directory, canonical
or extension. If it's not in this table, the dashboard and tooling don't
know about it.

| You want to know... | Look in |
|---|---|
| **Scope, principles, hard constraints, criticality rubric** | `.agent/PROJECT-SCOPE.md` |
| **Current state, in-flight work, next session plan** | `.agent/PROJECT-STATE.md` (this file) |
| **All ratified design decisions** | `.agent/DECISIONS/` (one file per decision; index in `DECISIONS/README.md`) |
| **Open check-ins awaiting input** | `.agent/CHECKINS/` (at root; archived live in `CHECKINS/ARCHIVED/`) |
| **Generated audit / inspect / sweep reports** | `.agent/REPORTS/` |
| **Ratified work-structure (milestone→task tree, depends-edges, history)** | `.agent/ROADMAP.md` (canonical when present; Active/Loose/Backlog/Shipped, per D-0050) |
| **Committed work ready now (derived frontier)** | `.agent/TODO.md` (generated from ROADMAP by `roadmap-render.sh`; never hand-edited) |
| **Unratified ideas** | `.agent/IDEAS/` (one file per idea, with `ARCHIVED/`, per D-0028) |
| **Threat models (per security-critical path)** | `.agent/SECURITY/` (e.g. `auth-path-threat-model.md`) |
| **[Extension: add rows for project-specific tracked surfaces]** | `.agent/[RESEARCH/ / NOTES/ / SPECS/ / etc.]` |

Extensions only "exist" in the tracking system if they appear in this
table. The dashboard reads this table to know what to render.

---

## 2. Active milestone

**Active = ROADMAP `## Active`** → see `.agent/ROADMAP.md` (if the project
uses ROADMAP). One-line pointer only: name the active milestone and its
ready/blocked frontier. Per-task DoD (`done-when:`) and progress live in
ROADMAP — do **not** duplicate the DoD checklist here (D-0050 dissolved the
old lockstep-with-SCOPE mandate, a Principle-7 violation).

**Milestone:** M2 — Session discovery + launch; see `ROADMAP.md` `## Active`.
**Active blockers:** none — M1 (the privileged core + served socket) is complete;
M2's spawn path builds directly on it.

(Projects not using ROADMAP may keep a short DoD list here instead.)

---

## 3. Open check-ins

(Files at the root of `.agent/CHECKINS/` that are not yet archived.
Each represents a question awaiting your input.)

- `<date>-<slug>.md` — [one-line summary]

Or: "none" if no active check-ins.

---

## 4. (Dissolved per D-0050)

Cross-session deferred work no longer lives in a narrated §4 thread. Route
it by kind: deferred-but-committed → a `## Backlog` task in `.agent/ROADMAP.md`
(naming its trigger); unratified → `.agent/IDEAS/`; decided-but-unbuilt → a
`DECISION`. (Projects not using ROADMAP may retain a §4 list.)

---

## 5. Next session

Start M2 (session discovery + launch). First task: discover installed sessions
from `/usr/share/wayland-sessions` + `xsessions` (parse `.desktop` `Exec=`/`Name=`),
exposed over the existing IPC seam. Then the spawn path — where `privdrop::drop_to`
fires — with the live privilege-drop demo as the DoD.

---

<!-- Optional sections below — add as your project needs.
     The dashboard renders any section it finds; canonical sections
     (1-5) are guaranteed to exist. -->

## 6. Recent milestones (one-liner index, optional)

If the project uses ROADMAP, shipped-milestone history lives in ROADMAP
`## Shipped` (D-0050) — don't duplicate it here. Otherwise:

- **M[N]** ([YYYY-MM-DD]) — [one-line summary]. [Closed / in-progress.]

## 7. Known issues / current debt (optional)

[Carry-forward issues that aren't blockers but are tracked.]
