# Investigation — greeter black-strip on physical eDP panel (open)

**Status:** OPEN, active. Reboot-crossing. Flagged by CHECK-IN 0002.
**Read this before proposing anything.** Every test appends a dated VERDICT row.
Never re-run a RULED-OUT test; never re-derive a settled fact.

## Symptom (fixed)
- Black horizontal **strip** on the **physical eDP-1 panel**, only at the **door
  greeter** (cage/greetd idle login screen), before login.
- **grim screenshot of the greeter = CLEAN** — no strip in the composited buffer.
- **Absent** on the normal desktop session and in BIOS.
- HW: Dell Tiger Lake / Iris Xe, eDP-1, 1920x1080. Boot: Limine + linux-cachyos.

## Interpretation
grim reads the compositor buffer (clean) → the strip is **not in rendered
content**. It is introduced on the **scanout/display path** and is
**greeter-context-specific** (cage picks a different mode/plane layout than the
desktop compositor, which never shows it).

## RULED OUT — do NOT re-test
| Date | Hypothesis | How tested | Result |
|------|-----------|-----------|--------|
| 2026-07-02 | **PSR** (panel self-refresh) | `enable_psr=0` confirmed **live/off**, rebooted, looked | strip **persisted** → NOT the cause (owner-confirmed 2026-07-03) |
| 2026-07-03 | **Direct-scanout** | `WLR_SCENE_DISABLE_DIRECT_SCANOUT=1` env drop-in (`scratch/test-no-direct-scanout-v2.sh`), rebooted, looked | strip **persisted** → NOT the cause |
| earlier | **Overscan / PSR re-chase** | prior memory note | set aside |

## KEY DIAGNOSTIC GAP (root cause of the round-trips)
All captures so far (`scratch/psr-recon.txt`, live checks) were taken from the
**desktop session** — the screen that does NOT show the bug. We have **zero DRM
plane / CRTC / mode data from the greeter itself**. Until we dump DRM state
*while the greeter is on the panel showing the strip*, we are guessing.

## OPEN hypotheses (untested)
1. **Plane/CRTC geometry mismatch** — cage's primary plane smaller than / offset
   within the CRTC active area → uncovered CRTC region scans out black; grim
   captures the plane buffer (full), not the uncovered gap. **Top suspect.**
2. **FBC** (`i915.enable_fbc`) — same grim-clean/panel-dirty Intel artifact class.
3. **Mode selection differs** — cage picks a mode whose active area ≠ panel native.
4. **HW cursor / overlay plane** leftover.

## NEXT ACTION — STAGED, awaiting one reboot
Auto-dump chosen (works without a second machine, no live coordination).
`scratch/install-greeter-drm-capture.sh` installs a removable systemd oneshot
(`greeter-drm-capture.service`) that fires 8s after doord starts the greeter and
dumps to `/var/log/greeter-drm-latest.txt`: **atomic DRM `state`** (plane/CRTC
src+dst rects — tests hypothesis #1), `i915_display_info`, `i915_fbc_status`
(hypothesis #2), PSR sanity, connector modes.
Owner runs: `sudo bash scratch/install-greeter-drm-capture.sh` → reboot → let
greeter sit ~10s → login → `cp /var/log/greeter-drm-latest.txt scratch/`.
Revert lines are in the installer header. The service cannot affect boot.

## Test log (newest first)
- 2026-07-03 | process | staged greeter-context DRM auto-capture (installer in scratch); awaiting owner reboot to produce the first dump from the failing screen | pending
- 2026-07-03 | process | investigation moved into dotagent (REPORT + CHECK-IN 0002); PSR + direct-scanout confirmed ruled out; identified all prior captures were from the wrong (desktop) context — the greeter's DRM state has never been dumped | —
