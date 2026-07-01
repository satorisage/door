# Spec — per-scene control map + sky-mode completeness (classification audit)

**Date:** 2026-06-30 · **Status:** GRADUATED → DECISION-0013 / ROADMAP M8 (2026-06-30);
kept as M8's working control-map reference · extends
`2026-06-30-scene-controls-audit.md`. Source: `door-theme/src/skyshader.rs` scene fns.

Rule applied (user): **keep** anything shared/internal that wouldn't mean anything to a
user as a dial; **expose + label** everything that *defines* a scene's look, grouped per
`sky_mode` so the controls read like that scene's blueprint. Defaults = current values
(out-of-box look unchanged). naga-gated per change.

## KEEP (shared / internal — never exposed)
- Base sky gradient direction/structure (`smoothstep(0,1,uv.y)`), the `u.time` clock,
  `fbm`/`hash21` noise internals + octave-loop counts (3), and fiddly falloff/sharpness
  exponents (e.g. bolt `7000`, star `90`) — feel-tuning, not user-legible.
- Cursor parallax + reduced-motion already exist as global knobs.

## Recurring axes (the per-scene groups are built from these, labeled per scene)
Almost every scene reduces to **Speed · Density/Count · Primary colour(s) · Intensity**
(+ a couple of scene-uniques like sun/moon position). Same underlying uniform slots,
**labeled to the scene** (e.g. Speed → "Lightning rate" / "Fall speed" / "Grid speed").

## Per-scene EXPOSE map (label ← current constant)

- **Aurora:** Curtain colour 1 (`0.18,1,0.66`) · Curtain colour 2 (`0.62,0.30,1`) ·
  Drift speed (`0.12`) · Band count (`3`) · Intensity (`0.6`). *keep:* ray-shimmer math.
- **Storm:** Lightning rate (`0.8`) · Strike chance (`0.32`) · Cloud density (c1/c2) ·
  Bolt colour (`0.85,0.9,1`) · Flash brightness (`0.55,0.6,0.8`). *keep:* bolt jag/decay.
- **Rain:** Fall speed (`5.0`) · Density (`0.93`) · Slant (`4.0`) · Rain colour
  (`0.55,0.62,0.72`) · Overcast sky colours. *keep:* layer count, streak shape.
- **Snow:** Fall speed (`0.9`) · Density (`0.86`) · Sway (`0.6`) · Flake size (`18`) ·
  Sky colours. *keep:* layer count.
- **Meteor:** Count (`10`) · Speed (`0.16`) · Direction angle (`-0.6,0.55`) · Trail
  length (`26`) · Meteor colour (`0.85,0.92,1`). *keep:* per-meteor RNG.
- **Moon:** Moon X/Y (`0.32,0.30`) · Size (`0.16`) · Phase speed (`0.06`) · Moon colour
  (`0.93,0.93,0.86`) · Halo (`0.28`). *keep:* maria fbm, terminator math.
- **Synthwave (pilot):** Grid speed (`1.2`) · Grid density (`0.55`) · Perspective
  (`0.6`) · Grid colour (cyan) · Grid glow (`0.7`) · Sun size (`0.22`) · Sun stripes
  (`120`) · Sun bloom (`0.25`) · Horizon (`0.56`) · Sky colour 1/2 (purple→pink).
- **Fog:** Drift speed (`0.02`) · Density (`0.45`) · Fog colour (`0.66,0.70,0.78`) ·
  Star visibility. *keep:* bank count.
- **Plasma:** Morph speed (`0.5`) · Scale (`8.0`) · Colour set / hue · Contrast.
- **Fire:** Rise speed (`2.0`) · Flame height (`0.22`) · Flame colour (`1.6,0.45,0.08`) ·
  Tip colour (`1,0.9,0.5`). *keep:* fbm warp scales.
- **Water:** Ripple speed (`0.6`) · Scale (`5.0`) · Caustic colour (`0.45,0.95,1`) ·
  Depth colours · Intensity (`0.85`). *keep:* domain-warp passes.

~5–10 dials/scene, all currently hardcoded. Architecture: shared scene-param uniform
pool (scenes are exclusive → reuse ~3–4 `vec4`s), packed by `sky_mode` in `from_theme`
from named per-scene config keys (`synthwave_grid_speed`…); settings shows one
per-scene group at a time.

## Sky-mode completeness (users can't author modes → ship the meaningful set)
Current: auto, seasonal, aurora, storm, rain, snow, meteor, moon, synthwave, fog,
plasma, fire, water. **Gaps worth adding** (curated, not exhaustive):
- **Day** + **Night** as explicit fixed modes — `day_sky`/`night_sky` already exist;
  just add selectable variants (near-free).
- **Solid / Minimal** — palette gradient, no animation (calm / low-power). Near-free.
- **Sunset / golden hour** — warm gradient + low sun (new shader).
- **Galaxy / Milky Way** — star band + nebula sweep (new).
- **Matrix / code rain** — green digital rain (new; popular "terminal" login look).
- **Earthy / landscape family** (new) — a coherent natural sub-set:
  - **Mountains** — parallax ridgeline silhouettes over a gradient sky (+ sun/moon).
  - **Forest** — layered pine/tree silhouettes, drifting mist, optional fireflies.
  - **Ocean** — sea horizon: animated water surface below + sky above + sun/moon glint
    (distinct from the existing underwater **water** caustics).
- *Optional later:* Hyperspace/warp stars, Lava-lamp blobs, Rainbow.

**Chosen set to ship (M8):** Day, Night, Solid, Sunset, Galaxy, Matrix, Mountains,
Forest, Ocean — i.e. all of the above (user: "all + a little earthy").
