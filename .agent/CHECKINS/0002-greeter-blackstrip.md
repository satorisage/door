# CHECK-IN 0002 — Greeter black-strip investigation (Material, open)

**Status:** open — active investigation, reboot-crossing.
**Criticality:** Material (cosmetic panel artifact on the pre-auth greeter; not a
security or boot blocker, but owner-visible and unresolved).

## The thread
Black strip on the physical eDP-1 panel, **only at the door greeter**, clean in
grim, absent on desktop/BIOS. Full detail + test log:
→ `.agent/REPORTS/2026-07-03-greeter-blackstrip-investigation.md`

## Ruled out (do not re-test)
- **PSR** — `enable_psr=0` confirmed live, strip persisted (owner-confirmed 2026-07-03).
- **Direct-scanout** — `WLR_SCENE_DISABLE_DIRECT_SCANOUT=1`, strip persisted.

## Why it kept stalling
Every capture was taken from the **desktop session** — the screen that doesn't
show the bug. The greeter's DRM plane/CRTC/mode state has **never** been dumped.

## ROOT CAUSE FOUND — 2026-07-03 (dump captured)
Greeter DRM dump (`scratch/greeter-drm-latest.txt`) settles it: active scanout
plane 1A (cage fb) is **1920×1045** while CRTC pipe A drives the panel at
**1920×1080** → bottom **35px** uncovered → black strip. grim reads cage's full
buffer, so screenshots stay clean. Hyp #1 confirmed; hyp #2 (FBC) rejected (FBC
disabled). Full detail in the REPORT.

## Remaining (downgrades this to a fix task, no longer a diagnosis mystery)
Sub-question: **why cage allocates a 1045-tall buffer** (not in `door*` config —
lives in cage/wlroots). Next probe: cage-side `WAYLAND_DEBUG`/`WLR_*` logging or
cage/wlroots version + output-mode check. Consider whether to file this upstream
vs. force cage to a full-height output.

## Cleanup owed
`scratch/install-greeter-drm-capture.sh` header has the revert lines — the
oneshot service can be removed now that the dump is in hand.
