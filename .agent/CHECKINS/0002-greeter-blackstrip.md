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

## Next action (blocks progress)
Dump DRM state **while the greeter shows the strip**. Routing question for owner:
does a second machine reach this box over SSH (capture live, zero persistent
change), or should we install an auto-dump systemd oneshot that fires after doord
starts the greeter?
