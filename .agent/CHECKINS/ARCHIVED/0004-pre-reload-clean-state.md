# CHECK-IN 0004 — pre-reload state snapshot (clean, not blocking)

**Opened:** 2026-07-03 · **Status:** OPEN (resume anchor) · **Criticality:** none —
informational. No pending decision; a fresh session can read this and just continue.

Owner is reloading (reboot). Unlike CHECK-IN 0003, **there is no in-flight
uncommitted work** — everything is committed and pushed. This is a bookmark.

## State at reload
- **HEAD = `6a6a86c`, origin/master == master, working tree CLEAN.** All pushed.
- **Every milestone M0–M9 is COMPLETE with no open legs.** M4 closed today
  (door-settings `pkexec` Save verified on hardware, `scratch/m4-save-verify.sh`
  PASS: `/etc/door/greeter.toml` sha changed, `root:root` `644`, diff = the real edit).
- **v0.1.8 released** (tag `v0.1.8` → `1c4e525`) — the keyboard-layout + battery
  indicators (opt-in, off by default). ROADMAP `## Shipped` backfilled through v0.1.8.

## What shipped this session (all on master, pushed)
- **D-0018 greeter indicators** — `show_kb_layout` (local xkb read) + `show_battery`
  (local sysfs read), both default-off, TCB-neutral; verified headless laptop +
  desktop (`scripts/verify-indicators-desktop.sh`). Released v0.1.8.
- **cc batch closed** (CHECK-IN 0003 archived) — Agent B's settings-UX/import-export/
  `clock_tz` integrated (`14580a9`); the 5 owner decisions filed as **D-0018**.
- **M4 complete** — the save leg.

## Open threads (all owner-paced / optional — nothing blocking)
1. **No-network red-team engagement** — owner-run (CRTO/OSCP) to *prove* `No network,
   ever` holds. Scoped: `.agent/SECURITY/no-network-verification-engagement.md`.
2. **Per-monitor wallpaper** — documented primary-output-only v1 bound; D-0006/D-0007
   supersession parked, revisit ~2026-07-17.
3. **Session lock screen** — captured as an IDEA (`.agent/IDEAS/2026-07-03-session-lock-screen.md`),
   parked. Graduates via DECISION + scope amendment if pursued.
4. **M-F** — parked entirely (owner directive).

## Owed cleanup (owner-run, on genny)
- Remove the WAYLAND_DEBUG drop-in (owed since v0.1.7):
  `sudo rm /etc/systemd/system/doord.service.d/30-wayland-debug.conf && sudo systemctl daemon-reload`

## Resume
Nothing to pick up mid-flight. A fresh session should read PROJECT-STATE (top block)
+ ROADMAP `## Active`, then either take an open thread above or await direction.
Archive this check-in once read.
