# Project State

**Last updated:** 2026-07-03 (**cc batch CLOSED — CHECK-IN 0003 resolved/archived.**
Agent B's `delegate/cc-settings-docs` (M8 settings-UX grouping, M7-E import/export,
M7-D `clock_tz` key + procedural/palette docs) was **verified green** (build + 20
tests + clippy; byte-identical greeter defaults; no new IO/exec/privileged surface)
and **integrated to master `14580a9`** (`--no-ff`); full-workspace re-verify passed
(doord 39 / theme 16 / settings 4 + others green, clippy clean). The **5 owner
decisions** are filed as **D-0018** (pre-auth stance round 2, supersedes D-0012's
indicator clause in part): keyboard-layout + battery indicators **authorized**
default-off (build tasks in ROADMAP `## Loose`); user-list **parked**; network
indicator **declined** — **`No network, ever` reaffirmed absolute**, and in its
place a **red-team no-network verification engagement** is commissioned
(`.agent/SECURITY/no-network-verification-engagement.md`, owner-run CRTO/OSCP);
per-monitor wallpaper ships the **documented primary-output-only v1 bound**, with a
D-0006/D-0007 supersession **parked to revisit ~2026-07-17**. **Owner actions owed:**
(1) `git push origin master` (integration not auto-pushed), (2) M4's last leg
`bash ./scratch/m4-save-verify.sh` on genny, (3) the WAYLAND_DEBUG drop-in cleanup
below still stands. Prior black-strip context below.)

**[history] Last updated:** 2026-07-03 (**greeter black-strip FIXED + on-panel confirmed** — the ~35px
bottom band was sctk-adwaita client-side decorations (`HEADER_SIZE=35`) drawn because iced left
`decorations: true` under cage; fix = `decorations: false` in `door-greeter/src/app.rs`, commit
`95eee0a`. CHECK-IN 0002 → RESOLVED/ARCHIVED; disposition **fixed**. **Release staged: v0.1.7**
(`dist/release-notes/v0.1.7.md`) — cut it with `scripts/release.sh 0.1.7`. Owner cleanup owed:
`sudo rm /etc/systemd/system/doord.service.d/30-wayland-debug.conf && sudo systemctl daemon-reload`.
Prior milestone context below.)

**[history] Last updated:** 2026-07-02 (**M5 COMPLETE** — Tier 4 Landlock validated on `genny` and shipping
enforce. `dist/systemd/doord.service` now sets `Environment=DOORD_LANDLOCK=enforce`; threat-model
note filed at `.agent/SECURITY/landlock-path-threat-model.md`. All four sandbox tiers now ship.)
**Active focus (2026-07-02): M5 CLOSED — supervisor fully sandboxed (spawner + seccomp + Landlock).**
Tier 4 per **DECISION-0017** (Binding): `hardening::apply_landlock` installs a best-effort ABI-V1
Landlock ruleset over the `SUPERVISOR_RO_PATHS`/`SUPERVISOR_RW_PATHS` seed (RO: `/usr /etc /proc
/sys /run`; RW: `/run/doord /dev`), supervisor-only after `apply_seccomp`, gated behind
`DOORD_LANDLOCK={off|enforce}` (`DOORD_NO_SANDBOX=1` kill-switch). Landlock has no `SCMP_ACT_LOG`
permissive mode, so it was validated enumerate-then-enforce: a **`genny` `DOORD_LANDLOCK=enforce`
boot ran a full login → logout → recycle → re-login cycle CLEAN with the shipped path seed**
(no widening needed; `scratch/genny-tier4-validate.sh` PASS — ruleset engaged, zero EACCES/audit
denials). Owner-run runtime act, recorded per D-0037. 39 unit tests + a fork-isolated confinement
test green; clippy clean. **Next = ship it: bump version + release (0.1.5 → 0.1.6, first build
carrying Tier 4 Landlock enforce). Open hardening idea: tighten the `/dev` grant to `/dev/dri` +
`/dev/tty*` leaves (N2, backlog).**

**[history] Active focus (2026-07-01): M5 Tier 2 — pre-forked spawner.** Reconciled to master HEAD
`9c9e1e1` after a ~3-milestone doc drift (this file had been frozen at 2026-06-27). Reality:
**M8** (per-scene authoring controls + mode completeness, D-0013) and **M9** (global
GPU-budget level, D-0014) are **COMPLETE and on master**; **M4**'s Done-when is **met on
hardware** (real-VT PASS 2026-07-01 — themed greeter → password → PAM → session; lone open
leg = the door-settings *save* to `/etc/door` via `pkexec`, design-verified only). The live
frontier is **M5 Tier 2 part 2** (DECISION-0015): the pre-forked spawner is wired behind
`DOORD_SPAWNER` (HEAD `9c9e1e1`) and its **login path is now HARDWARE-VALIDATED** — a
`DOORD_SPAWNER=1` systemd drop-in on `genny` booted doord in spawner mode and ran a **real PAM
login (`stephen`) into a `plasma` session over the spawner** (2026-07-01, journal `-b` 10:45 CDT,
session stable 9h+). This was a **runtime/operational act, not a commit**, so D-0037's
commit-boundary sync never fired — hence it went undocumented until reconciled here from the live
machine (same class of gap as the earlier ~3-milestone drift). **The clean logout/teardown leg is
now ALSO hardware-validated (2026-07-01, journal `-b` on `genny`):** the `plasma` session exited
status 0 at 20:58:43, the worker was reaped, the greeter recycled (relaunched pid 72418, VT freed),
and a fresh PAM login succeeded over the same supervisor (`doord[910]`, alive 10:45→20:59 ~10h, no
restart) — including a clean auth-failure path (one password typo → worker died, supervisor
untouched → retry succeeded). Verified no orphaned workers (`ps --ppid 910` = only the idle
pre-forked helper `914`; live session worker `setsid`s into its own scope by design). **So the full
login → logout → reap → recycle → re-login cycle runs over the spawner.** **The shipped default is
now flipped:** `dist/systemd/doord.service` ships `Environment=DOORD_SPAWNER=1` (reversible — unset
falls back to legacy in-lineage spawn; the flag/legacy path is retained on purpose until Tier 3/4
are also hardware-proven). **Remaining on M5:** Tier 3 seccomp / Tier 4 Landlock on the supervisor —
now unblocked, since session-spawn has left doord's sandboxed lineage. **Tier 3 is
staged in 3 increments** (log-only-before-enforce per D-0015; each needs a genny boot to
validate): **(1) greeter reroute — CODE COMPLETE and HARDWARE-VALIDATED (2026-07-01, HEAD
`86739f6` on `genny`):** the greeter-through-spawner + concurrent-reaper build booted and a full
login → logout → greeter-recycle → re-login cycle ran clean over the spawner (owner-reported
runtime act, not a commit — same D-0037 gap class as the other spawner validations, recorded here
from the live run). The greeter compositor
(`cage`) was forked directly by the supervisor and re-forked on every logout, so a supervisor
seccomp filter would confine it — greeter lineage had to move off the supervisor first (owner
decision 2026-07-01: route the greeter through the spawner). Landed: the spawner now serves the
**greeter worker** too (`REQ_SPAWN_GREETER`) and manages **concurrent** children (greeter + session
worker coexist during auth) via a non-blocking SIGCHLD-interrupt reaper — the old serial blocking
reap would have deadlocked; the supervisor tracks the greeter by control-fd EOF (mirroring the
session worker) + pid-signal, `WorkerLoginFactory` is now stateless with `begin(greeter, spawner)`,
and cage no longer inherits the control fd (CLOEXEC in `run_greeter`). Behavior-preserving; 30
doord unit tests green (incl. new `serves_concurrent_children_of_both_kinds`), clippy clean.
**Then (2) seccomp in `SCMP_ACT_LOG` (boot, grep journal for denials); (3) flip to enforce.** The milestone narrative below (M2/M3/M6/M4/M7) is retained as
**history/context, not current state**.

---

**[history] M2 COMPLETE and live-confirmed (2026-06-26).** The full
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

**M6 COMPLETE (2026-06-27) — packaging + reversible install, proven on hardware.**
door installs from a `PKGBUILD` (`door 0.0.0-2`) **disabled by default**, never
clobbering the active DM; doord owns the greeter lifecycle (D-0008), sessions are
tied to doord's lifetime, and the seat is freed by killing the compositor's process
group (D-0009). Full done-when met live: package install → `enable --now` → real
greeter login into Plasma → **two-command TTY revert** (`disable --now doord` +
`enable --now sddm`) restores the previous DM cleanly. The live-enable lockout
postmortem found and fixed **nine** root causes (RC1–RC9), all proven on hardware —
the last, RC9, is what made the revert actually work (the compositor was squatting
the seat's DRM master behind a self-respawning supervisor; doord now kills its
process group). Full detail in `.agent/REPORTS/2026-06-26-m6-lockout-postmortem.md`;
condensed in ROADMAP `## Shipped`; 26 unit + 3 integration tests green, clippy clean.

**Now active: M4 — the beautiful greeter (started 2026-06-27).** Direction (owner):
**wallpaper + frosted card**, **config-driven theme engine + a built-in default**.
Landed: the theme schema in a shared **`door-theme`** crate (TOML at
`$DOORD_GREETER_CONFIG` → `/etc/door/greeter.toml` → `/usr/share/door/greeter.toml`
→ built-in default; strict parse, `#rrggbb[aa]` colors, missing-asset degrade); the
themed greeter `app.rs` (wallpaper `stack`, translucent glass card with breathing
accent edge + soft shadow, accent sign-in, clock+date, MesloLGS Nerd Font, launch
fade-in, **ambient twinkle + shooting-star canvas**, Tab focus); the packaged
default (`dist/door/greeter.toml` + the genny-config Tokyo Night **comet**
wallpaper); and a standalone **`door-settings`** editor (iced, D-0010 — Preview
launches the real greeter on a draft, Save writes via `pkexec`; a QML KCM was
rejected to stay Rust/out-of-TCB). **Then (2026-06-27):** the sky + card spinner
moved to per-pixel **GPU WGSL shaders** (no canvas banding), the **day** variant got
a real 3-D atmosphere (sun + parallax sun-lit clouds + haze), and the **control
surface expanded ~20 knobs across 3 tiers** (D-0011) — five door-settings tabs with
per-control Help and an `--expert`/Advanced gate; every key documented in
`greeter.toml` and defaulting to the prior look. A `naga` test parse-validates both
shaders (no GPU). **Nested visual check PASS (2026-06-28)** — the real greeter under
nested `cage` + `grim`, day & night, confirmed the full themed surface (animated sky,
glass card, clock, Meslo font, per-variant spinner, day/night auto-switch) and
door-settings' live preview (frames in `scratch/m4shots/`). Found+fixed a packaging
gap: the default font `MesloLGS Nerd Font` wasn't a PKGBUILD dependency → added
`ttf-meslo-nerd`, bumped `pkgrel 1→2` (ready for a `0.1.0-2` AUR re-publish).
**Real-VT confirmation PASS (2026-07-01):** user installed the new binary + enabled the
systemd unit, rebooted straight into door on the real VT — themed greeter rendered,
password entered, PAM authed into the session. The nested-render prediction held; the M4
Done-when is now met on hardware. Remaining M4 leg: interactive door-settings *save*
(`pkexec`-write to `/etc/door`, design-verified only); day sun-bloom already dialed.
Criticality Material (pre-auth UI). Workspace tests green, clippy clean. See ROADMAP `## Active`.

**M8 — ✅ COMPLETE (2026-06-30, on master; superseded the "not yet built" note below).**
Shipped across 10 commits (`f9bda7a`→`3d0567b`): scene-param uniform pool, per-`sky_mode`
packing, named per-scene keys, door-settings per-scene groups, and the full new mode set
(Day/Night/Solid, Mountains/Forest/Ocean/Pyramids, Sunset/Galaxy/Matrix). 16/16 door-theme
tests incl. naga gates green. Original ratification note (historical):

**M8 ratified (2026-06-30, D-0013).** Per-scene
authoring controls + sky-mode completeness. A source audit found the scene shaders are
palette-blind with no exposed dials; owner's reframe = expose each scene's *exact*
defining constants as labeled per-scene controls (shared exclusive-scene uniform pool,
named keys packed by `sky_mode`, per-scene settings group, defaults preserve the look),
keeping shared/internal constants unexposed. Plus round out the mode set (users can't
author modes): Day, Night, Solid, Sunset, Galaxy, Matrix, + earthy family (Mountains,
Forest, Ocean). Classification/EXPOSE map: `.agent/IDEAS/2026-06-30-scene-controls-spec.md`;
task tree in ROADMAP `## Backlog` → M8. Build order: shared pool → synthwave pilot →
roll-out → new modes. TCB-neutral; naga-gated. **Next session: start M8 foundation +
synthwave pilot.**

**Released v0.1.1 to the AUR (2026-06-30).** Tagged `v0.1.1`, live on the AUR (pkgver
0.1.1, pkgrel 1). Ships the full **M7** program (groups A–E) + the **door-settings
overhaul** — see ROADMAP `## Shipped`. M7 groups A–E complete (C composition, D
clock/logo/type, E functional-UX all landed); F items stay DECISION-gated (D-0012:
caps-lock reclassified TCB-neutral + built; user-list/indicators/`.wgsl`/video parked).
Notable fix this cycle: the settings **black-screen** on reduced-motion presets — a
zero-size `Space` as the top-level `stack!` base collapsed the layout (looked black,
hit randomly via the dice); fixed with a Fill-size base, greeter unaffected, regression
test added. Workspace tests + clippy green.

**M7 — Graphics & greeter depth (started 2026-06-29).** New milestone (authorized "a-F
it is"); full inventory in `.agent/IDEAS/2026-06-29-graphics-capability-audit.md`,
sequenced in ROADMAP `## Backlog` → M7. **Group A complete** (`7d179bf`/`0d83a22`/
`29ba879`): 17 hardcoded shader constants lifted to theme knobs (sun, glow center, day
haze, stars, sky comet, clouds, spinner orbit) — settings stays 100%-mapped. **Group B
complete** (`a21392e`…`701d40b`): a `sky_mode` selector + **12 scenes** (aurora, storm,
rain, snow, meteor, moon, synthwave, fog, plasma, fire, water) each = one enum variant +
one WGSL branch (picker auto-reads `SkyMode::ALL`), plus **seasonal auto-select** (scene
by month; door-theme stays calendar-free, caller passes it) and **palette-pin** (explicit
scenes force the readable night card). All naga-gated + clippy clean + verified live
under cage. **F3 weather-sky** stays the helper-file design (greeter never networks; "No
network, ever" upheld); F1/F2 (custom `.wgsl`, video) remain DECISION-gated. Pushed
through `f9474ea`; A's last + B's tail (`a9cb4aa`,`5b71614`,`701d40b`) local. Next in M7:
C (composition) / D (clock-logo-type) / E (functional-UX). Criticality Material.

**v0.1.0 release prep (2026-06-27):** cut version `0.0.0 → 0.1.0`; AUR-ready PKGBUILD
(builds from the tagged GitHub source tarball, not the working tree), MPL-2.0 `LICENSE`,
honest README (early-alpha, Wayland-only, Arch+systemd). **X11 sessions are filtered from
the picker by default** (`DOORD_ALLOW_X11=1` to override) — door starts no X server, so
offering them would be a login that can't succeed; real X11 support is deferred past v1.
Sharing-without-`.agent`: **[SUPERSEDED — see below]** the strategy landed as
**gitignore + keep-on-disk** — `.agent/`, `CLAUDE.md`, and `scratch/` are gitignored
(never tracked...), and the `.gitattributes export-ignore` lines are kept as a
redundant tarball safety net.

**[CORRECTION 2026-07-02]** The gitignore-`.agent` claim above is **stale/wrong for
current reality.** `.agent/` is **tracked and committed** — `.gitignore` excludes only
`*.db`, `/target/`, `/pkg/`, `/scratch/`, `CLAUDE.md`, `vendor/`, and debug captures
(**not** `.agent/`), and `git log -- .agent` shows 12+ commits (e.g. `e2c84b8`
"docs(agent): …"). So governance **does** go to the public GitHub repo, under a
`docs(agent):`/`feat(...)` commit convention, and the earlier "gitignore + keep-on-disk"
plan was **not** the one that stuck. `CLAUDE.md` and `scratch/` remain gitignored as
stated; only the `.agent/` half of the claim is corrected. (Old text kept struck-through
above for the audit trail, per "don't edit history.")

**Released to the AUR (2026-06-28):** `v0.1.0` is live on the AUR
(`https://aur.archlinux.org/packages/door`). Public-history audit confirmed clean:
governance was never tracked, so the `door-scrub`/`git-filter-repo` scrub prepared
by `release.sh` was unnecessary and was discarded. README updated from "(once
published)" to the live package link; `scripts/install-local.sh` (working-tree
dev-install counterpart to the PKGBUILD) committed.

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

**Milestone:** **none active — M0–M9 all shipped/complete.** M5 (all four sandbox
tiers, incl. Tier 4 Landlock enforce) closed 2026-07-02 (D-0017); the earlier
"Next = Tier 4" note here was stale and is corrected. See `ROADMAP.md` `## Active`
for the current **polish + verification frontier** (not a milestone): M4's lone
door-settings *save* leg (owner-run harness), the D-0018 default-off greeter
indicators (keyboard-layout + battery, in `## Loose`), the no-network red-team
verification engagement (`.agent/SECURITY/`), and the parked per-monitor-wallpaper
supersession revisit (~2026-07-17). M-F stays parked (owner directive).

(Projects not using ROADMAP may keep a short DoD list here instead.)

---

## 3. Open check-ins

(Files at the root of `.agent/CHECKINS/` that are not yet archived.
Each represents a question awaiting your input.)

**none** — CHECK-IN 0003 (cc batch) resolved + archived 2026-07-03 (all 5 owner
decisions filed as D-0018; Agent B integrated to master).

---

## 4. (Dissolved per D-0050)

Cross-session deferred work no longer lives in a narrated §4 thread. Route
it by kind: deferred-but-committed → a `## Backlog` task in `.agent/ROADMAP.md`
(naming its trigger); unratified → `.agent/IDEAS/`; decided-but-unbuilt → a
`DECISION`. (Projects not using ROADMAP may retain a §4 list.)

---

## 5. Next session

**M5 Tier 3 COMPLETE — enforce shipped as the `doord.service` default (2026-07-01).** Increment 3
is done: the full-allowlist binary was restored (undoing the 2b armed control build), the machine-local
drop-in flipped to `DOORD_SECCOMP=enforce`, and a **full login → logout → recycle → re-login cycle ran
clean under enforce on `genny`** (owner-reported "all clear, full test"). With that hardware
confirmation, `Environment=DOORD_SECCOMP=enforce` is now **baked into the shipped
`dist/systemd/doord.service`** (placed with the `DOORD_SPAWNER=1` block, since the code only applies the
filter in spawner mode — `main.rs:94`). `SeccompMode::Enforce` → `SCMP_ACT_ERRNO(EPERM)` — a stray
unlisted syscall fails locally rather than killing the supervisor (`hardening.rs:116`). Reversible per
D-0016: unset for no filter, `DOORD_SECCOMP=log` to re-observe, `DOORD_NO_SANDBOX=1` to force off for
lockout recovery. Workspace tests green (76), clippy clean. **Remaining on M5: Tier 4 — Landlock paths**
on the supervisor (same post-`fork_spawner` step, unblocked by the spawner split).

**[history] Tier 3 increment 2b (genny seccomp log-run) — VALIDATED.** 2a integrated to master (merge
`1eb8009`). On `genny` the full-allowlist Log binary ran the full cycle with an **empty denial harvest**,
and the **positive control passed** (`scratch/tier3-poscontrol.sh`: dropping `write`/`writev` produced
the expected `type=1326` records in Log mode), proving the `SCMP_ACT_LOG` capture path surfaces denials
and `SUPERVISOR_ALLOWLIST` is complete — no widening needed.

**[history] Tier 3 increment 1 (greeter reroute) — hardware-validated 2026-07-01** (HEAD `86739f6`,
committed `39a7267`). The greeter routes through the spawner + concurrent reaper; a full login →
logout → greeter-recycle → re-login cycle ran clean over the spawner on `genny`. The greeter
compositor no longer forks off the supervisor, so the supervisor is now confinable (unblocked 2a).
The validation + install commands are in `scratch/tier3-validate-increment1.sh`.

**[history] M5 Tier 2 — full cycle validated + shipped default flipped.** The
pre-forked spawner is wired behind `DOORD_SPAWNER` and its **full cycle is
hardware-validated** on `genny` (2026-07-01, journal `-b`): login → clean logout (status 0) → worker
reap → greeter recycle → re-login, all over the spawner, no orphans, one clean auth-failure retry
along the way. The shipped `dist/systemd/doord.service` now sets `Environment=DOORD_SPAWNER=1`
(reversible flip — unset falls back to legacy in-lineage spawn; kept on purpose until Tier 3/4 are
also proven). Still open: **Tier 3 (seccomp allowlist) / Tier 4 (Landlock)** on the supervisor — the
unit's Tier-1 comment block explains these were unsafe until session-spawn left doord's lineage,
which the spawner now does, so they are unblocked. Minor open leg from M4: exercise the door-settings *save* (`pkexec`-write to
`/etc/door`).

**[history] M3 — finish the live end-to-end** (done 2026-06-26). The greeter is built
and **verified running**: run directly against a wlroots compositor (wayland-0) it
connects to doord, handshakes, lists sessions, renders, and holds the connection — no
crash. What remained was a human driving auth → start (since completed).

Two findings from the first live attempt (historical):
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
