# DECISION-0009 — a session's lifetime is tied to doord; doord frees its seat by killing the compositor process group

**Status:** Binding
**Date:** 2026-06-27
**Ratified:** 2026-06-27
**Project:** door
**Relates to:** D-0008 (doord owns the greeter lifecycle) — this extends the same
ownership to the *teardown* edge: doord owns freeing the seat, not just handing it off.

## Context

M6's revert path failed in practice (postmortem RC9): booted into door and logged
into Plasma, `systemctl stop doord && systemctl start sddm` left a bare blinking
cursor and sddm never appeared (`HELPER_TTY_ERROR` loop). Hardware diagnosis showed:

- A door-spawned graphical session **outlives doord**. Its compositor runs under
  the per-user systemd manager (`user@<uid>.service`), not in doord's cgroup or the
  logind session scope, so stopping doord cannot reach it — and it keeps **seat0's
  DRM master**, blocking any next login manager (sddm, *or* doord's own re-greeted
  cage) from acquiring the GPU.
- `loginctl terminate-session` does **not** free it: that drops the logind session
  but leaves the compositor orphaned under `user@<uid>` still holding the master.
- The compositor runs behind a **supervisor** (`kwin_wayland_wrapper`) that
  *respawns* it; killing the bare DRM-holder pid just bounces DRM (the desktop sees
  a monitor unplug/replug) and never frees the seat.

Conventional DMs sidestep this by running each user session on its own VT
(greeter on tty1, session on tty2+); door's single-VT handoff (D-0008) does not.
Web research confirmed the separate-VT path is the unsolved, buggy frontier even
for sddm/gdm (VT leaks, reactivation failures), not a clean win.

## Decision

**A door session's lifetime is tied to doord.** doord does not try to preserve a
user's desktop across its own stop/restart/crash; instead it **frees its seat** so
the next login manager (or its own re-greet) always gets a clean GPU. The lever is
killing the **process group** of whatever holds the seat's `/dev/dri/card*` —
which takes the compositor's supervisor down with it, so nothing respawns
(`free_seat()` in `ipc.rs`). It fires at three points:

1. **Seat-claim before each greet** — clear any squatter so cage can always take
   the seat (also fixes the RC7 restart/crash-respawn wedge at its source).
2. **On teardown** — a `SIGTERM`/`SIGINT` during a live session sets a flag the
   interruptible session-wait loop acts on: free the seat, restore the VT, exit —
   so `stop doord; start sddm` reverts cleanly.
3. **Panic hook** — a doord panic frees the seat before the process dies.

Scope: **every doord exit** (clean stop/disable and crash). The desktop is *not*
preserved across them — accepted, because the compositor is the squatter and
cannot be cleanly preserved across a login-manager teardown. This matches the
common DM expectation that restarting the login manager ends the graphical session.

## Consequences

- New `free_seat()` primitive + an interruptible session-wait (`wait_for_session`)
  + a teardown flag + a panic hook in `ipc.rs`; `SessionChild::try_wait` in
  `spawn.rs`. Single-seat assumption: targets every `/dev/dri/card*` KMS node
  (render-only `renderD*` excluded); a multi-seat host would need per-seat scoping.
- Reverting from door to another DM under a live session now works without a
  reboot; a doord restart/crash under a live session re-greets cleanly instead of
  wedging on a held GPU.
- Tradeoff: unsaved desktop work is lost on a doord stop/restart/crash.
- Residual gap: a `SIGKILL`/power-loss of doord runs no cleanup, but the next
  start's seat-claim (point 1) clears the orphan. A session-bound leftover like
  `plasmashell` (`PartOf=graphical-session.target`, `Restart=on-failure`) has no
  compositor to draw to, holds no GPU, and is stopped by its own StartLimit —
  invisible and harmless, so `free_seat` does **not** try to sweep it. (A
  `loginctl terminate-seat` sweep was tried 2026-06-27 and reverted: fully ending
  the seat's sessions leaves the VT session-less, and logind's `autovt` then races
  a getty `login:` prompt onto the seat console before doord re-greets — a visible
  flash, strictly worse than the invisible leftover. Revert confirmed flash-free on
  hardware 2026-06-27: a doord restart under a live session re-greets with no getty
  prompt.)
- Proven on hardware 2026-06-27 (postmortem RC9): doord seat-claimed a stuck
  compositor and greeted; `stop doord; start sddm` reached the sddm login with no
  flicker/respawn. 26 unit + 3 integration tests green, clippy clean.

## Alternatives considered

- **Sessions on their own VT** (conventional DM): the session would survive a
  doord restart with no contention. Rejected for M6: a substantial change to the
  D-0008 single-VT handoff, and web research shows it is the unsolved, buggy
  frontier even in mature DMs (VT leaks, reactivation failures).
- **`loginctl terminate-session` on teardown**: proven on hardware to leave the
  compositor orphaned under `user@<uid>` still holding the master — insufficient.
- **Kill the bare DRM-holder pid** (not the group): proven on hardware to trigger
  the supervisor's respawn (monitor unplug/replug loop) — insufficient.
- **Document clean-logout-first as the only supported revert** (no killing in a
  root daemon): viable but leaves the naive `stop doord; start sddm` broken-looking;
  the owner chose to make it work.
