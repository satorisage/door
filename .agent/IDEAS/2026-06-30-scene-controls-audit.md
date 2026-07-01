# Idea — sky scenes are uncontrollable & palette-blind (audit)

**Date:** 2026-06-30 · **Status:** GRADUATED → DECISION-0013 / ROADMAP M8 (2026-06-30) ·
**Lens:** ui-ux + honest-bounds
**Trigger:** user noticed the synthwave grid/sun has "no clear way to know what controls
those or how that config even happens." Source-grounded audit of `door-theme/src/skyshader.rs`.

## Finding (systemic, not synthwave-only)

Source audit of the 11 fixed `sky_mode` scenes — which theme uniforms each reads:

- **Fully hardcoded (read no palette, only `u.time`):** storm, rain, snow, **synthwave**,
  plasma, fire, water — **7 of 11**.
- **Partial (read only `bg_bot` + `glow`):** aurora, meteor, moon, fog — 4 of 11.
- **Scene-specific controls exposed as knobs:** **none, for any scene.**

`synthwave_sky` (the lead example) is 100% constants: `horizon 0.56`; sky `purple/pink`;
sun radius `0.22`, stripe freq `120`, glow `9.0`; **grid scroll `time*1.2`**, density
`0.55`, perspective `0.6`, grid colour cyan `(0,0.9,1.0)`, intensity `0.7`.

## Consequences (the UX gap the user hit)

1. **Palette-blind:** with a hardcoded scene active, editing Colors/Accent/Glow does
   nothing to the sky → the preset's own colours are ignored, which is confusing.
2. **No scene controls + no discoverability:** the signature motion/look of each scene
   (synthwave grid speed, storm lightning rate, rain density, fire height…) has no knob
   and no UI hint that it's procedural. The settings panel gives no map from
   "scene = X" → "these controls affect it."
3. Contrast: M7-A made the **default day/night** sky "100%-mapped" (every constant a
   knob); the B scenes were explicitly exempt (self-contained branches). So this is a
   known scope boundary now surfaced, not a regression.

## Options (pre-decision — pick a lane, then it graduates to a DECISION + ROADMAP task)

- **A. Palette-wire the scenes** *(low–med, biggest consistency win, stays knob-free):*
  derive each scene's colours from `glow_color`/`accent`/`background`/`comet` instead of
  hardcoded constants. Presets then actually recolour every scene. naga-gated per change.
- **B. A few scene-shared knobs** *(med):* generic `scene_speed` / `scene_intensity`
  uniforms every scene's motion+strength reads (synthwave scroll, rain density, fire
  height all scale off them). ~2 knobs cover all 11 scenes; settings gets 2 sliders.
- **C. Per-scene control disclosure in settings** *(med–high, best discoverability):*
  when a `sky_mode` is selected, reveal its relevant controls / a "this scene is
  procedural — tuned by X, Y" note. Solves "how do I even configure this."
- **D. Document-only** *(low):* in the Help tab + greeter.toml, state which scenes are
  palette-driven vs fully procedural, so expectations are honest (Principle 6).

Recommended starting lane: **A + D** (recolour scenes from the palette so presets work
as expected, and document the procedural-vs-palette split). B/C are follow-ons if we
want true per-scene tuning.

## Chosen direction (user reframe, 2026-06-30): expose the EXACT per-scene dials

Not generic knobs / not auto-palette — surface the actual constants that authored each
scene (the M7-A "every constant a knob" treatment, applied to the B scenes). Architecture:

- **Shared scene-param uniform pool** (~3 `vec4` = 12 floats). Scenes are mutually
  exclusive, so the active scene reinterprets the same slots as its own parameters —
  ~12 slots cover all 11 scenes instead of ~90 dedicated uniforms.
- **Lift each scene's hardcoded constants into the pool**, current values as defaults →
  out-of-box look unchanged until a dial moves.
- **Per-scene settings group**: selecting a `sky_mode` reveals that scene's exact
  controls (synthwave → grid speed/density/perspective/horizon/colour, sun
  size/stripes/glow, sky colours). One group visible at a time.
- **Config:** named per-scene keys (`synthwave_grid_speed = …`) packed into the shared
  slots by `sky_mode` in `from_theme`.
- Likely its own milestone (**M8 — per-scene authoring controls**); **pilot on
  synthwave** first to lock the pattern, then roll scene-by-scene. naga-gated per change.

## Out of scope / constraints
- TCB-neutral (more uniforms / self-contained shaders), sits under Tenet 1 like A–E.
- Every shader edit stays gated by the existing `naga` parse+validate test.
