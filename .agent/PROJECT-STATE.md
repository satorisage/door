# Project State

**Last updated:** 2026-06-26
**Active focus:** **M2 is COMPLETE and live-confirmed (2026-06-26).** The full
logind handoff works: pam_systemd registration (D-0004) with a **per-login session
worker as the logind leader** (D-0005) — the daemon re-execs itself (`worker.rs`)
into a short-lived process that owns the PAM transaction, is the logind leader,
`setsid`s + takes the seat's VT, drops privilege, runs the session in the
sanitized allowlist ∪ PAM env, then closes the session and exits. The daemon never
enters a session scope; greeter framing never reaches the worker (`O_CLOEXEC`).
**Multi-login live run passed:** two back-to-back logins registered sessions 17
then 18 (leader = the worker, each in its own scope), ran as `uid=1000` on
`/dev/tty4` with `XDG_SESSION_*`, **each closed cleanly**, and the daemon stayed in
`system.slice/doord-m2.service`. `cargo test` green (30). M2 committed + merged +
pushed (`4470f88`).

**M3 complete (2026-06-26)** — `door-greeter` (Iced + iced_layershell, D-0006)
verified live against a wlroots compositor: connect → list → render → auth →
`Start`. In `## Shipped`. 34 tests green workspace-wide; M2 + M3 merged to `master`
(`cc43654`, not yet pushed).

**Now active: M6 — packaging + reversible install. Criticality: Critical**
(lockout domain — revert-first; install disabled-by-default, never clobber the
existing DM, tested TTY revert before enabling). Gating decision **resolved
(D-0007): cage + plain-`iced` fullscreen toplevel** (greeter reworked). **Handoff /
re-greet orchestration built (D-0008):** doord owns the greeter lifecycle — a new
greeter worker (`worker::run_greeter`) opens a passwordless greeter-class logind
session (PAM `door-greeter`) for seat access and forks `cage -- door-greeter`;
`ipc::serve` is the login loop (greet → serve → on `Start` terminate the greeter
and wait for the VT to free **before** the session takes it (S14) → wait session →
re-greet, with crash-loop backoff). `door-greeter.service` removed (doord owns the
greeter); packaging is now `doord.service` + `dist/pam.d/{doord,door-greeter}` +
`dist/sysusers.d/door.conf` + `PKGBUILD`/`door.install` (installs disabled). 34
tests green, clippy clean. **The only remaining M6 work is the live install
validation** (Critical, hardware, revert-first): `makepkg -si` → enable on a spare
machine/VT → real login + logout + re-greet, tested TTY revert in hand. The handoff
is built but unproven on hardware. See ROADMAP `## Active`.

**Lockout postmortem (2026-06-26):** the first live `enable --now` locked the
machine — greeter couldn't reach the socket and `Ctrl+Alt+F3` was dead (chroot to
recover). Two root causes fixed (`.agent/REPORTS/2026-06-26-m6-lockout-postmortem.md`):
**RC1** `/run/doord` was `0700 root:root`, unreachable by the greeter user —
`ipc::bind` now group-owns it `0750`; **RC2** recoverability — `doord.service`
conflicts the DM + `StartLimit*`, and `ipc::serve` now *gives up* after a short
greeter crash-streak (restores the VT to `VT_AUTO`/`KD_TEXT` and exits cleanly so
it can't thrash the GPU/console). 34 tests green. Still unproven on hardware —
re-test stays revert-first with the escape path confirmed *before* enabling.

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

**Milestone:** M6 — Packaging + reversible install; see `ROADMAP.md` `## Active`.
**Active blockers:** the M6 gating decision — host compositor + greeter surface
type (cage lacks layer-shell). **Criticality: Critical** (lockout domain;
revert-first). M1/M2/M3 are complete.

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

**M3 — finish the live end-to-end.** The greeter is built and **verified running**
(2026-06-26): run directly against a wlroots compositor (wayland-0) it connects to
doord, handshakes, lists sessions, renders, and holds the connection — no crash.
What remains is a human driving auth → start.

Two findings from the first live attempt:
- The greeter uses `KeyboardInteractivity::Exclusive` (correct for a real login
  screen) — running it in a *live desktop* grabs the keyboard. A **dev mode**
  (`DOORD_GREETER_DEV=1`) now renders a small floating surface with on-demand
  keyboard so the full flow can be smoke-tested nested without lockout:
  `DOORD_GREETER_DEV=1 DOORD_SOCKET=/run/doord-demo.sock ./target/debug/door-greeter`
  (Ctrl-C the launching terminal to quit). Use `/tmp/doord-m3-greeter.sh` to bring
  doord up first.
- **`cage` (this Arch build) has no `wlr-layer-shell` support** (no
  `zwlr_layer_shell` symbols) — it cannot host the layer-shell greeter, nested or
  on a VT. The production greeter needs a layer-shell-capable host compositor
  (sway / weston / labwc), **or** we reconsider a plain-iced fullscreen toplevel
  (which works under cage). This is a deployment-compositor decision for M6 /
  possibly a D-0006 follow-up — flagged, not yet decided.

Residual: daemon-side `Power` is still stubbed; M2's N2 session lifecycle and the
M5 hardening pass remain.

Open doc item: correct D-0003 H5's ordering text (`initgroups → setresgid →
setresuid`); see the note in `ROADMAP.md`.

Open doc item: correct D-0003 H5's ordering text to match the reviewed `privdrop`
code (`initgroups → setresgid → setresuid`); see the note in `ROADMAP.md`.

Cleanup from the live runs (optional): `sudo systemctl stop doord-m2` (the test
service is still running), `rm -f /run/doord-demo.sock`, `/tmp/doord-m2-*`. The
installed `/etc/pam.d/doord` is now the complete production file (the M1 stub is
backed up at `/etc/pam.d/doord.m1-stub.bak`) — safe to keep.

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
