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

Output-mode check done (2026-07-03): panel eDP-1 advertises **only 1920×1080** —
no 1045 mode exists, so cage is *allocating* a short buffer, not mis-selecting a
mode. cage 0.3.0. WAYLAND_DEBUG is already wired via
`doord.service.d/30-wayland-debug.conf` (DOORD_GREETER_CMD, since env_clear strips
it from doord's own env).

### WAYLAND_DEBUG handshake captured (2026-07-03) — sub-question localized
Read-only from this boot's journal, no restart. The client-side handshake is
**fully correct at 1920×1080** — the 1045 truncation is invisible at the Wayland
protocol layer, so it lives *below* it in cage/wlroots' DRM plane allocation:
- `wl_output#12.mode(1, 1920, 1080, 60049)` — cage advertises the full panel mode.
- `xdg_toplevel#20.configure(1920, 1080, …)` — cage tells the greeter to render
  **full-height** (not short).
- `wl_output#12.scale(1)` + `wl_surface.set_buffer_scale(1)` — no scaling/viewport.

**Rules out:** output-mode mis-selection AND client-side short sizing. The greeter
renders 1080; the scanout plane is 1045. → the short buffer is allocated inside
cage/wlroots' DRM backend independent of the client-negotiated size. Consistent
with the scanout-plane finding. Next: cage/wlroots DRM-backend allocation path
(upstream-side), or force cage to a full-height scanout — no more client-side probes.

**Capture-script fix:** `greeter-wl-capture-boot.sh` filtered `journalctl -u doord`
(the *service unit*) and caught only 8 lines — cage/greeter run in a **separate
session scope** (PIDs 938/969, syslog id `doord`, different cgroup than
`doord.service`). Use `-t doord` (syslog identifier), not `-u doord` (unit). The
83k lines of protocol traffic were in the journal the whole time.

### DEAD END — do NOT restart doord to capture the trace (2026-07-03)
`scratch/greeter-wl-trace.sh` (restart doord → trace from line 1) is **retired.**
Restarting doord while cage holds DRM master **wedges the panel**: SIGKILL does
*not* release the GPU synchronously (`GPU still held after SIGKILL`), so the new
greeter can't become DRM master → `starting the session failed: Operation not
permitted (os error 1)` → greeter dies, backs off 3×, doord goes `inactive (dead)`,
orphaned cage/door-greeter leave the display **locked**. Confirmed 2026-07-03 (run
over SSH; recovered by reboot). The restart was never necessary anyway.

### Correct capture — READ-ONLY, from this boot's journal
The handshake (wl_output.mode + xdg/layer `.configure` sizes) fires ONCE at greeter
start and is **already in the journal** from boot — "buried under ~84k lines" ≠
unretrievable; `grep` pulls it out regardless of position. Use
`scratch/greeter-wl-capture-boot.sh` (`journalctl -u doord -b 0 | grep <handshake>`,
no restart) → `scratch/greeter-wl-handshake.txt`. Only caveat: if journald
rate-limited the frame flood and dropped the early lines, the script says so and
points at the drop check.

## Cleanup
- DRM-capture oneshot: **done** — `greeter-drm-capture.service` + `/usr/local/sbin`
  helper already removed (verified 2026-07-03, unit not found). Root-cause dump
  `scratch/greeter-drm-latest.txt` retained as evidence.
- Post-probe: once `greeter-wl-trace.sh` has the handshake, remove the debug
  drop-in (it floods the journal): `sudo rm
  /etc/systemd/system/doord.service.d/30-wayland-debug.conf && sudo systemctl
  daemon-reload && sudo systemctl restart doord`.
