# Idea: expand the greeter control surface

**Raised:** 2026-06-27 · **Status:** RATIFIED 2026-06-27 — build all 3 tiers; gate Tier 3
behind a `--expert` flag in door-settings. (File a DECISION at first commit.)

## Build plan (staged; each stage builds + tests green, defaults reproduce today's look)
1. **comet_color slice** — one field end-to-end (schema→merge→render→from_theme→WGSL),
   fixes the sky-comet-reuses-spinner-color nit. Proves the pipeline.
2. **Schema helpers** — add `merge_f32`/`merge_bool` closures to cut the per-field
   boilerplate before the bulk add.
3. **Sky controls** (Tier 1+2) — sky_glow, star_density, star_twinkle, comet_enabled,
   comet_frequency, day_cloud_amount/speed, sun_x/y (+Tier2 glow pos/falloff, nebula,
   comet path/dir/decay, sun size/color, cloud shadow). Pack into shader uniforms.
3b. **Spinner controls** — spinner_size, spinner_pulse_speed (+T2/3 ring/glint/nucleus/bloom).
4. **Card/greeter controls** — card_shadow blur/opacity, accent_breathing speed/amount,
   field_radius/button_radius, error_color, fade_duration_ms, clock_24h (app.rs).
5. **Settings UI** — widgets for Tier 1 across reorganized tabs (Sky/Spinner/Card/Behavior);
   `--expert` reveals an **Advanced** tab/sections with Tier 2+3.
6. **Docs + defaults** — every key documented in dist/door/greeter.toml; DECISION filed.

Per-variant (night/day) vs shared classification decided per field at implementation;
shader controls pack into vec4 uniform arrays (std140). Flat `Theme` fields kept
(consistent with the existing hand-rolled pattern, Principle 8).

---


The greeter's look is driven by `door-theme::Theme` (TOML at `greeter.toml`, edited in
`door-settings`). A diligent sweep of the shaders + UI found ~70 hardcoded values that
*could* become controls. Below they're triaged into tiers — not all are worth exposing
(most internal shader constants are design choices, not knobs). Full raw inventory with
`file:line` cites is in the conversation that produced this; the curation is here.

Legend: **[settings]** = worth a door-settings widget · **[toml]** = power-user TOML-only
key · **[skip]** = leave hardcoded (rarely useful / easy to break the look).

---

## Already exposed (the current surface)
Per-variant: `background, card (+alpha), field, accent, foreground, muted,
spinner_comet, spinner_track, spinner_glow, spinner_trail`. Shared: `wallpaper, logo,
font, corner_radius, card_width, show_clock, animate, day_start, day_end, spinner_speed`.

---

## Tier 1 — high value, user-meaningful (recommend [settings])

### Sky
- **sky_glow** — night indigo glow/haze strength (`params.w`, now 0.50). [settings]
- **star_density** — twinkle threshold (now 0.90; lower = more stars). [settings]
- **star_twinkle_speed** — pulse-rate scale (now `0.8 + h2*3.6`). [settings]
- **comet_enabled** — toggle the drifting background comet. [settings]
- **comet_frequency** — period between sweeps (now 9.5 s). [settings/toml]
- **day_cloud_amount** — cloud coverage/alpha (new day shader; now ~0.5–0.65). [settings]
- **day_cloud_speed** — cloud drift rate (new; now ~0.013–0.027). [settings]
- **sun_position** — day sun x/y (now upper-left 0.24,0.18). [toml]

### Spinner (card emblem)
- **spinner_size** — currently fixed 52 px; scale or center it. [settings]
- **spinner_pulse_speed** — head breathing rate (now 3.0). [toml]

### Card / UI
- **card_shadow** — drop-shadow blur+opacity (now blur 34, α 0.45). [settings]
- **accent_breathing_speed** + **_amount** — card hairline pulse (now 1.1 rad/s, 0.18–0.34). [toml]
- **field_radius / button_radius** — independent of card radius (now all 10). [toml]
- **error_color** — NEW: tint the status line red on auth-fail (today always `muted`). [settings]

### Behavior
- **clock_24h** — 12/24-hour clock format. [settings]
- **fade_duration_ms** — launch fade-in length (now 384 ms). [toml]

---

## Tier 2 — power-user knobs (recommend [toml] only)
- Sky: `glow_position`, `glow_falloff`, `nebula_amount`, `nebula_speed`, comet
  `start/end`, comet `tail_direction`, comet `tail_decay`, comet `color` (own key, not
  reusing `spinner_comet`), day sun `size`/`color`, cloud `shadow_color`.
- Spinner: `ring_intensity`, `glint_enabled`/length, `nucleus_falloff`, coma
  `bloom_strength` (merge the 3 halo layers into one knob).
- Card: shadow `color`/`offset`, `logo_size`, `field_padding`, `form_spacing`,
  per-element `text_size` (clock/date/fields/status).

## Tier 3 — leave hardcoded [skip]
Per-octave fbm weights, dither strength, star glint axis falloffs, button hover-lighten,
tab/subcard styling, the 16-layer canvas-glow constants (dead `sky.rs`). These shape the
*identity* of the look; exposing them mostly lets a user break it.

---

## Notes / dependencies
- Each new **shader** control = one more field in `Uniforms` (watch std140 alignment)
  + `from_theme` wiring + a `Theme`/`ThemeFile` key. Cheap individually; pick a batch.
- New **per-variant** keys (e.g. cloud amount) need night+day in `render_pair`.
- A `comet_color` distinct from `spinner_comet` is the one *correctness* nit: the sky
  comet currently reuses the spinner's color, which is tuned for the card, not the sky.
- Biggest UX win outside aesthetics: **error_color** (real feedback on failed login).

---
**GRADUATED 2026-06-27 → DECISION-0011** (built through stage 6, committed). Archived.
