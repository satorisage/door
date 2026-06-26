# Project State

**Last updated:** 2026-06-25
**Active focus:** M2 (session discovery + launch). Discovery shipped; the
**session-spawn path is built, tested, and live-confirmed** — `Start` is
auth-gated and bound to the PAM-authenticated user; the daemon forks, runs
`privdrop::drop_to` in `pre_exec` (drop-or-abort), sanitizes the env, and execs
the discovered `Exec` (protocol → v2, `Response::Started`). **Live root run on
2026-06-25 passed:** spawned child reported `uid=1000(stephen) gid=1000(stephen)`
with the user's groups, sanitized `PATH`, `LD_PRELOAD` unset — the drop sub-task's
DoD is met. **One M2 task remains: logind seat/VT/session wiring.**

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

One M2 task remains: **logind seat/VT/session wiring** — `setsid`/controlling
tty, VT switch, the logind session registration (`XDG_SESSION_*`), and session
lifecycle (re-greet, respawn policy). Its threats are deferred in
`SECURITY/session-spawn-threat-model.md` §5 (N1/N2) and modeled when it lands.
This completes M2 (its Done-when names the seat/VT wiring as the last piece).

Open doc item: correct D-0003 H5's ordering text to match the reviewed `privdrop`
code (`initgroups → setresgid → setresuid`); see the note in `ROADMAP.md`.

Demo leftovers from the live run (optional cleanup): `/etc/pam.d/doord` (a valid
service — door's production default; safe to keep), `/run/doord-demo.sock`,
`/tmp/door-demo-sessions/`.

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
