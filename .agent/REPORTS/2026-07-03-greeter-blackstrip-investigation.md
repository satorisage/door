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

## ROOT CAUSE — CONFIRMED 2026-07-03 (greeter-context DRM dump)
Source: `scratch/greeter-drm-latest.txt` (dumped 06:12:29, greeter on panel).
**Hyp #1 CONFIRMED — plane is 35px shorter than the mode:**
- `crtc[171] pipe A` active, `mode: "1920x1080"` @60 (full panel height 1080).
- Only active scanout plane `plane 1A`: `fb=633 allocated by = cage`,
  `size=1920x1045`, `crtc-pos=1920x1045+0+0`. All other pipe-A planes `fb=0`.
- **1080 − 1045 = 35px** at the panel bottom covered by **no plane** → scans out
  black. grim reads cage's full compositor buffer, never this uncovered gap →
  clean screenshot. Desktop compositor allocates a full 1080 buffer → no strip.
- Every earlier symptom row is now mechanically explained by this single fact.

**Hyp #2 REJECTED:** `i915_fbc_status: FBC disabled: pixel format not supported`
on all DRI nodes → FBC is off, not the cause.
Hyps #3/#4 subsumed: the mode IS native 1080; the defect is the plane BUFFER
height (1045), not mode selection or a leftover cursor/overlay.

## OPEN sub-question — which buffer is 1920×**1045**, and why?
Narrowed 2026-07-03:
- Stack: **cage 0.3.0 / wlroots 0.20** (cage version flag is `-v`, not `--version`).
- eDP-1 (`card1-eDP-1/modes`) exposes **only 1920x1080** — there is **no 1045
  mode**. The dump already showed CRTC pipe A mode = 1080, so **mode selection is
  correct**; 1045 is a *scanout-buffer* size, not a mode. Branches #3 ("cage picks
  a short mode") is dead.
- doord builds the greeter command internally (only `DOORD_GREETER_USER` is
  exposed as env) → the next probe's `Environment=` inheritance is the right hook.

Two live branches: (a) cage's **composite output buffer** is 1045 (cage/wlroots
0.20 bug), or (b) the **greeter client buffer** is 1045 and cage direct-scans it
out (note: the earlier direct-scanout-disable test was reverted before this dump,
so direct scanout may have been active when 1045 was captured — iced/door-greeter
could be attaching a 1045-tall surface). **Next probe** disambiguates: staged
`scratch/install-greeter-wayland-debug.sh` (`WAYLAND_DEBUG=1`, env-only) logs the
`xdg_toplevel` configure size cage sends and the buffer size the greeter attaches.
1080-configure + 1045-attach → greeter bug; 1045-configure → cage bug.

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
- 2026-07-03 | **DRM dump** | greeter-context capture produced (`scratch/greeter-drm-latest.txt`) | **ROOT CAUSE FOUND**: active plane 1A fb = 1920×1045 vs CRTC mode 1920×1080 → bottom 35px uncovered → black strip. Hyp #1 confirmed, hyp #2 (FBC) rejected. Open sub-question: why cage allocates 1045.
- 2026-07-03 | process | staged greeter-context DRM auto-capture (installer in scratch); awaiting owner reboot to produce the first dump from the failing screen | pending
- 2026-07-03 | process | investigation moved into dotagent (REPORT + CHECK-IN 0002); PSR + direct-scanout confirmed ruled out; identified all prior captures were from the wrong (desktop) context — the greeter's DRM state has never been dumped | —
