# DECISION-0014 — global GPU-budget level (Lite / Moderate / High / Bonkers)

**Status:** Binding
**Date:** 2026-06-30
**Ratified:** 2026-06-30
**Project:** door
**Relates to:** D-0011 (greeter control surface) — adds one more global control;
D-0013 (per-scene controls) — orthogonal axis: D-0013 tunes *what a scene looks like*,
this tunes *how much GPU the whole render spends*; `reduced_motion` (existing) — the
motion floor this sits above.

## Context

door's animated sky is a per-pixel fragment shader that runs every frame at full
display resolution — the dominant GPU cost (≈500M invocations/sec at 4K/60). Heavy
scenes (plasma, water, fire, meteor) and the frosted-card blur pass compound it. door
runs on login screens, often on modest iGPUs, and the operator can't currently trade
visual richness for power/thermals/battery. The owner asked for a single **global**
quality dial — "lite / moderate / high / bonkers" — explicitly **not** per-preset:
it wraps the whole render, it is not part of the scene/preset path.

## Decision

Add one global enum key **`gpu_level`** (`lite | moderate | high | bonkers`, default
`high`) that maps to a bundle of render-cost levers. It is a top-level `greeter.toml`
key + a single door-settings dropdown, applied globally (never per-scene, never
per-preset, never merged from a preset).

Tier → lever mapping (target):

| Tier | render scale | fps cap | shader quality | card_blur | grain/vignette/extra star layers |
|---|---|---|---|---|---|
| Lite | 0.5× | 30 | low | off | off |
| Moderate | 0.75× | 60 | medium | off | trimmed |
| High (default) | 1.0× | 60 | full | on | on |
| Bonkers | 1.0× | uncapped | full + MSAA | on | on |

**Levers, by GPU impact:**
- **Render scale** — render the sky to a smaller offscreen texture, upscale. ~4× less
  fragment work at 0.5×. Highest impact; needs an offscreen pass in iced's `shader`
  widget (the one fiddly piece).
- **Frame-rate cap** — throttle the redraw loop; linear power saving.
- **Shader quality uniform** — a `quality` value the scenes read to cut `fbm` octaves
  and loop iteration counts (rain/snow layers, meteor count, aurora curtains, nebula
  samples). Internal-only; not a per-scene user dial (stays unexposed per D-0013's
  "keep internals internal" rule).
- **Blur toggle** — `card_blur` is a second full-screen pass; off at low tiers.
- **grain / vignette / star-layer count** — minor per-pixel extras, trimmed low.

**Build order (this ratification authorizes the MVP now):**
- **MVP (build now):** frame-rate cap + shader-quality uniform + blur/extras gating.
  Covers most of the win at low/medium effort and low risk.
- **Fast follow (queued):** render-scale (the offscreen downsample) — biggest single
  win, deferred only because the iced offscreen pass warrants careful, unrushed work.

## Consequences

- New milestone **M9** in `ROADMAP.md` (MVP task + render-scale follow-on task).
- `door-theme`: a `GpuLevel` enum + `gpu_level` key; a `quality` uniform packed into
  the shader uniforms; scenes consult it to scale loop/octave counts; defaults at
  `high` keep the current look byte-for-byte (quality uniform at full = today's
  constants). `naga` test continues to gate every shader change.
- `door-greeter`: the redraw loop honors the fps cap; blur/extras gated by tier.
- `door-settings`: a single global "GPU level" dropdown (not in the per-scene groups).
- TCB-neutral (render-budget only; no new pre-auth attack surface) — sits under Scope
  Tenet 1 like the rest of the control surface.
- Honest-bounds (Principle 9): `greeter.toml`/Help document what each tier trades, and
  that `high` is the byte-faithful default; render-scale's deferral is stated, not silent.
