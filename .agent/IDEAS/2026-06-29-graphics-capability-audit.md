# Graphics capability audit + future ideas (2026-06-29)

Source-verified against `door-theme/src/{lib.rs,skyshader.rs}` and
`door-settings/src/main.rs`. Three parts: (1) what the graphics engine does, (2)
mapping audit — knobs vs settings, (3) what else we could do (tagged by pre-auth
TCB risk, since the greeter is a pre-login surface).

## 1. What we've built (the engine)

GPU per-pixel WGSL (iced `shader` widget over wgpu), shared by greeter + settings:

**Sky shader (`WGSL`)**
- Night: vertical gradient, radial sky-glow/haze, fbm nebula, 3 parallax twinkling
  star layers (per-layer pulse + cross-glint on the brightest), drifting teardrop
  comet on an InOutSine sweep + pause, ordered dither (kills 8-bit banding).
- Day: zenith→horizon haze gradient, bloomed sun, 2 parallax sun-lit cloud layers
  with fbm domain-warp + self-shadowing, horizon haze.
- Day/night is a uniform flag; all colors flow from the `Theme`.

**Spinner shader (`SPIN_WGSL`)** — premultiplied-additive comet emblem: orbit ring →
exponential arc trail → coma halo → white-hot nucleus + pip → 4-point star glint,
radial edge fade. `hot = clamp(glow)` ties whiteness to the glow knob.

**Card / chrome (iced widgets, not shader)**: translucent frosted card, accent
hairline border (breathing), drop shadow, rounded fields w/ accent-on-focus,
full-width accent button, clock+date, logo-or-spinner with optional backdrop tile,
launch fade-in, power row.

**Theme system**: 42 editable knobs + `is_day` variant flag + day-window; night/day
auto-select by local clock; TOML load/merge (strict, partial-merge, graceful
degrade); presets (user + packaged); round-trip render.

## 2. Mapping audit — are all controls in settings?

**Yes — 100% of theme knobs are exposed.** All 42 `Theme` knobs route through
door-settings `build()` and have a control across 5 tabs (Colors/Sky/Spinner/Card/
Behavior) + per-control Help + an Advanced (`--expert`) tier for the 4 expert knobs.
`is_day` is the Night/Day toggle; day-window is in Behavior. Nothing in the schema is
unreachable. The settings control surface and the theme schema are in lockstep.

**The real gap is one level down:** the shaders contain many *hardcoded constants*
that are NOT theme fields, so they can't be exposed. These are "free" knobs — the
art already exists, it's just pinned:

| Hardcoded in shader | Could become knob | File:loc (skyshader.rs) |
|---|---|---|
| Sun position `vec2(0.24,0.18)` | `sun_x` / `sun_y` | day_sky |
| Sun warm color + core color | `sun_color` | day_sky |
| Sun halo size/intensity (16.0 / 0.38) | `sun_size` / `sun_intensity` | day_sky (just tuned) |
| Sky comet path + direction `(0.92,-0.39)` | `comet_angle` | sky comet block |
| Comet pause (2.5s) | `comet_pause` | sky comet block |
| Comet teardrop width `0.010 + t*0.10` | `comet_width` | sky comet block |
| Star layer count (3), star size `*70` | `star_layers` / `star_size` | star loop |
| Nebula drift speed | `nebula_speed` | night block |
| Cloud shadow color, lit color | `cloud_shadow` / `cloud_lit` | cloud_layer |
| Cloud layer count (2), fbm octaves (5) | `cloud_layers` | day_sky / fbm |
| Night glow center `vec2(0,-0.06)` | `glow_x`/`glow_y` | night block |
| Day gradient mix (zenith .60 / horiz .30) | `day_haze` | day_sky |
| Spinner orbit radius `R=0.30` | `spinner_orbit` | SPIN_WGSL |

## 3. What else we could do (beyond current capability)

### A. Cheap wins — lift existing shader constants to knobs
The table above. Each is a `Theme` field + uniform + settings slider/cell, no new
art. Highest value: **sun position/size/color** (day feels fixed right now),
**comet angle/frequency already partly knobbed**, **star size**, **cloud colors**.
TCB risk: none (same shader, more uniforms).

### B. New sky scenes / modes (new shader code)
A `sky_mode` enum beyond day/night: **rain, snow, aurora/northern-lights, fog,
storm+lightning, meteor shower (N comets), moon + phases, planets, synthwave grid,
plasma/fractal, fire, water caustics, solid/gradient-minimal.** Seasonal auto-select
(snow in winter) parallels the day/night clock trigger. TCB risk: low (self-contained
shaders; validated by the existing naga test).

### C. Composition / layout
- **Card placement** (center/left/right/bottom), alignment, multiple-monitor /
  per-output wallpaper.
- **True backdrop blur** behind the card (currently translucent only — a blur pass
  is the one thing iced won't give for free; doable as a shader sample of the sky).
- Card **gradient fill / border gradient / inner glow / grain / vignette**.
- **Animated day↔night transition** (sunrise sweep) instead of an instant variant
  flip — we already proved the crossfade looks great in the reddit clip.
- Cursor-parallax sky (mouse moves the star layers).

### D. Clock / logo / type
- Clock: **seconds, analog face, custom strftime format, multiple timezones,
  size knob, separate clock font**.
- Logo: **SVG, animated logo, distro auto-detect**; spinner **style alternatives**
  (ring/dots/bar/pulse, selectable).
- Font: **weight, global size scale, letter-spacing**.

### E. Functional / UX (greeter behavior, not just paint)
- **User list with avatars** (pick a user vs typing), **caps-lock / keyboard-layout
  / battery / network indicators**, localized strings.
- **Accessibility**: reduced-motion toggle (honor a "no animation" intent),
  high-contrast theme, large-text mode, color-blind-safe palettes.
- door-settings polish: a real **HSV/RGB color-picker** (vs hex text), **live preset
  thumbnails**, **randomize / "surprise me"**, **import/export & share presets**,
  **theme hot-reload** (watch greeter.toml).

### F. High-power, but weigh the pre-auth TCB cost (surface, don't default-yes)
- **Custom user `.wgsl` background** (Shadertoy-style): maximal flexibility, but runs
  admin-supplied shader code on the login screen — needs naga validation + resource
  bounds at minimum; still expands the pre-auth surface.
- **Video / GIF wallpaper** (mpv/gstreamer): pulls a media stack into the pre-auth
  TCB — heavy, decoder CVEs are a real class. Probably a no for v1 ethos.
- **Live weather-driven sky**: implies network (or a cached file written by a
  privileged helper) before auth. Prefer a file a logged-in helper drops, never the
  greeter dialing out.

**Guiding constraint:** A–E are mostly TCB-neutral (more uniforms / self-contained
shaders / unprivileged greeter UI). F trades attack surface for flair on the
pre-auth surface — exactly where door's whole thesis says be conservative. Any F
item should be an explicit, documented decision, off by default.
