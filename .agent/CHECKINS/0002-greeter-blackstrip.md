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

## Next action (STAGED — one owner reboot away)
Greeter-context DRM auto-capture is staged: `scratch/install-greeter-drm-capture.sh`
installs a removable, boot-safe oneshot that dumps DRM state 8s after the greeter
comes up. Owner: `sudo bash scratch/install-greeter-drm-capture.sh` → reboot →
let greeter sit ~10s → login → `cp /var/log/greeter-drm-latest.txt scratch/`.
Then read the atomic `state` plane rects (hyp #1) and FBC (hyp #2).
