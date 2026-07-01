# DECISION-0013 — per-scene authoring controls + sky-mode completeness (M8)

**Status:** Binding
**Date:** 2026-06-30
**Ratified:** 2026-06-30
**Project:** door
**Relates to:** D-0011 (greeter control surface / 100%-mapped invariant) — extends it to
the scene shaders; D-0012 (pre-auth surface stance) — F-items stay gated, this is
TCB-neutral; `.agent/IDEAS/2026-06-30-scene-controls-audit.md` +
`…-scene-controls-spec.md` (the source-grounded classification this ratifies).

## Context

M7-B added 12 fixed `sky_mode` scenes as self-contained WGSL branches; controllability
was out of scope. A source audit (spec file) found the gap: **7 of 11 scenes read no
theme palette at all, and none expose their scene-defining parameters** — so a user can
see a look (e.g. the synthwave grid/sun) with no discoverable way to tune it or even
know what configures it. Owner's reframe: don't add generic sliders or auto-palette —
**expose the exact dials that authored each scene**, labeled so the control surface
reads like that scene's blueprint. And because **users cannot author modes in the app**,
the built-in mode set must be comprehensive for what a user would meaningfully want.

## Decision

Open **M8 — per-scene authoring controls + mode completeness.** Two halves:

1. **Expose the scene-defining constants as labeled per-scene controls.**
   - Architecture: a **shared scene-param uniform pool** (~3–4 `vec4`s). Scenes are
     mutually exclusive, so the active scene reinterprets the same slots as *its*
     parameters (≈12 slots cover all scenes, not ~90 dedicated uniforms).
   - **Named per-scene config keys** (`synthwave_grid_speed = …`) packed into the pool
     by `sky_mode` in `from_theme`; defaults = current constants (out-of-box look
     unchanged). The exposed set per scene is the spec's EXPOSE map (~5–10 dials each).
   - **Settings:** a per-`sky_mode` control group, one shown at a time, labeled to the
     scene (Speed → "Lightning rate" / "Fall speed" / "Grid speed", etc.).
   - **Keep unexposed** (per owner's rule): shared/internal constants that aren't a
     legible user dial — gradient direction, `fbm`/`hash` internals + octave counts,
     fiddly falloff/sharpness exponents.

2. **Round out the mode set** with: **Day, Night, Solid** (near-free — `day_sky`/
   `night_sky` already exist; Solid = palette gradient, no animation), **Sunset**,
   **Galaxy/Milky Way**, **Matrix/code-rain**, and an **earthy/landscape family —
   Mountains, Forest, Ocean** (Ocean = sea-horizon, distinct from the underwater
   `water` caustics).

Build order **within** M8: shared pool → **synthwave pilot** (locks the pattern
end-to-end) → roll out remaining scenes → new modes. Every shader change stays gated by
the existing `naga` parse+validate test; per-scene controls keep the door-settings
100%-mapped invariant (D-0011) for the exposed set.

## Consequences

- New milestone **M8** in `ROADMAP.md` with a task tree (shared-pool foundation, the
  synthwave pilot, per-scene roll-out, the new modes, the per-scene settings UI, docs).
- `door-theme`: scene-param uniform pool + per-scene named knobs + `sky_mode`-keyed
  packing; each scene's WGSL reads the pool (defaults preserve the current look).
- `door-settings`: per-scene control groups surfaced by the selected `sky_mode`; the
  mode picker grows to the new set.
- TCB-neutral (more uniforms / self-contained shaders / unprivileged UI) — sits under
  Scope Tenet 1 like A–E; F-items (custom `.wgsl`, video) remain DECISION-gated (D-0012).
- Honest-bounds (Principle 6): Help/`greeter.toml` document which modes are procedural
  vs palette-driven and what each scene's dials do.
