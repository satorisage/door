//! The greeter's look (M4): a config-driven theme with a built-in beautiful
//! default. Shared by `door-greeter` (which renders it) and `door-settings` (which
//! edits it) — one source of truth for the schema and the colors.
//!
//! The greeter is the pre-auth surface, so the config is admin-controlled and
//! world-readable — never user-supplied at login time. It is loaded from the first
//! file that exists, each merged *over* the built-in [`Theme::default`] so a partial
//! file (just an `accent`, say) works:
//!
//! 1. `$DOORD_GREETER_CONFIG` — explicit path, for dev/testing
//! 2. `/etc/door/greeter.toml` — admin override
//! 3. `/usr/share/door/greeter.toml` — packaged default
//!
//! If none exist or one fails to parse, the built-in default stands (the greeter
//! must always render). Asset paths (wallpaper, font, logo) point at world-readable
//! files installed system-wide; a missing asset degrades to the solid background /
//! stock font rather than failing.

pub mod clock;
pub mod sky;
pub mod skyshader;

use serde::Deserialize;
use std::path::PathBuf;

/// Explicit config path override (dev/testing). Checked before the system paths.
pub const ENV_CONFIG: &str = "DOORD_GREETER_CONFIG";
/// The admin override path — where `door-settings` saves.
pub const ETC_CONFIG: &str = "/etc/door/greeter.toml";
/// Admin override, then the packaged default. First readable wins.
const SYSTEM_CONFIG_PATHS: [&str; 2] = [ETC_CONFIG, "/usr/share/door/greeter.toml"];

/// An 8-bit RGBA color, parsed from `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 0xff }
    }
    const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Parse `#rrggbb` or `#rrggbbaa` (case-insensitive). Returns `None` on any
    /// malformed input so the caller can keep the default and warn.
    pub fn parse(s: &str) -> Option<Color> {
        let hex = s.strip_prefix('#')?;
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        match hex.len() {
            6 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, 0xff)),
            8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Render as `#rrggbb` (opaque) or `#rrggbbaa` (with alpha) — the inverse of
    /// [`parse`](Self::parse), used when writing a config.
    pub fn to_hex(self) -> String {
        if self.a == 0xff {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Convert to an `iced::Color` (linear 0.0–1.0 components with alpha).
    pub fn iced(self) -> iced::Color {
        iced::Color::from_rgba8(self.r, self.g, self.b, self.a as f32 / 255.0)
    }

    /// As an `iced::Color` with the alpha scaled by `mult` (0.0–1.0) — drives the
    /// launch fade-in (the card and its text ramp up together).
    pub fn iced_alpha(self, mult: f32) -> iced::Color {
        iced::Color::from_rgba8(self.r, self.g, self.b, (self.a as f32 / 255.0) * mult)
    }
}

/// The sky scene the greeter renders. `Auto` keeps the day/night-by-clock behavior
/// (the default); the others force a specific animated scene regardless of time.
/// New modes are added here + as a branch in the sky shader (M7 group B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkyMode {
    /// Day or night by the local clock — the default behavior.
    #[default]
    Auto,
    /// Auto-pick a scene by the calendar season (snow in winter, meteors in summer…).
    /// Resolved to a concrete scene by the caller (which owns the clock); falls back
    /// to the day/night renderer if unresolved.
    Seasonal,
    /// Flowing aurora curtains over a night sky.
    Aurora,
    /// Dark churning clouds with periodic lightning flashes + a bolt.
    Storm,
    /// Overcast sky with falling rain streaks.
    Rain,
    /// Soft winter sky with drifting snowflakes.
    Snow,
    /// A frequent diagonal meteor shower over a night sky.
    Meteor,
    /// A large phased moon (slowly cycling) over a starfield.
    Moon,
    /// Retro synthwave: a striped sun over a neon perspective grid.
    Synthwave,
    /// Drifting fog banks over a muted sky.
    Fog,
    /// Demoscene plasma — swirling psychedelic color fields.
    Plasma,
    /// Rising flames from the bottom of the screen.
    Fire,
    /// Underwater light caustics rippling over a blue-green deep.
    Water,
}

impl SkyMode {
    /// Every mode, for the settings picker.
    pub const ALL: [SkyMode; 13] = [
        SkyMode::Auto,
        SkyMode::Seasonal,
        SkyMode::Aurora,
        SkyMode::Storm,
        SkyMode::Rain,
        SkyMode::Snow,
        SkyMode::Meteor,
        SkyMode::Moon,
        SkyMode::Synthwave,
        SkyMode::Fog,
        SkyMode::Plasma,
        SkyMode::Fire,
        SkyMode::Water,
    ];
    /// The shader selector value (0 = auto → the day/night renderer).
    pub fn shader_id(self) -> f32 {
        match self {
            SkyMode::Auto => 0.0,
            // Seasonal is resolved to a concrete scene before render; 0 = auto fallback.
            SkyMode::Seasonal => 0.0,
            SkyMode::Aurora => 1.0,
            SkyMode::Storm => 2.0,
            SkyMode::Rain => 3.0,
            SkyMode::Snow => 4.0,
            SkyMode::Meteor => 5.0,
            SkyMode::Moon => 6.0,
            SkyMode::Synthwave => 7.0,
            SkyMode::Fog => 8.0,
            SkyMode::Plasma => 9.0,
            SkyMode::Fire => 10.0,
            SkyMode::Water => 11.0,
        }
    }
    /// The lowercase name used in the config and the picker.
    pub fn name(self) -> &'static str {
        match self {
            SkyMode::Auto => "auto",
            SkyMode::Seasonal => "seasonal",
            SkyMode::Aurora => "aurora",
            SkyMode::Storm => "storm",
            SkyMode::Rain => "rain",
            SkyMode::Snow => "snow",
            SkyMode::Meteor => "meteor",
            SkyMode::Moon => "moon",
            SkyMode::Synthwave => "synthwave",
            SkyMode::Fog => "fog",
            SkyMode::Plasma => "plasma",
            SkyMode::Fire => "fire",
            SkyMode::Water => "water",
        }
    }
    /// Resolve a selector mode to a concrete scene. Only `Seasonal` changes — mapped to
    /// a scene by calendar month (1–12); every other mode passes through unchanged.
    pub fn resolved(self, month: u32) -> SkyMode {
        match self {
            SkyMode::Seasonal => match month {
                3..=5 => SkyMode::Rain,        // spring
                6..=8 => SkyMode::Meteor,      // summer (Perseids!)
                9..=11 => SkyMode::Fog,        // autumn
                _ => SkyMode::Snow,            // winter (12, 1, 2) + fallback
            },
            other => other,
        }
    }
    /// Which card palette a mode pins, if any. `None` = follow the day/night clock
    /// (Auto/Seasonal). Every explicit scene pins **night** (`Some(false)`): the dark
    /// card stays readable over any scene, and it avoids a light day-card landing over
    /// a dark scene (e.g. daytime + aurora). The light day palette is for the day sky.
    pub fn prefers_day(self) -> Option<bool> {
        match self {
            SkyMode::Auto | SkyMode::Seasonal => None,
            _ => Some(false),
        }
    }
}

impl std::fmt::Display for SkyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A single global GPU-budget level. Wraps the *whole* render — never
/// per-scene, never per-preset. Bundles the render-cost levers: shader detail
/// (fbm octaves), frame-rate cap, and whether the second-pass card blur is allowed.
/// `High` is the byte-faithful default (full octaves, today's look unchanged).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuLevel {
    /// Bare minimum — half the shader detail, 30 fps, no blur. For weak iGPUs / battery.
    Lite,
    /// Trimmed detail, 60 fps, no blur.
    Moderate,
    /// Full detail, 60 fps, blur on. The default — current look unchanged.
    #[default]
    High,
    /// Everything uncapped (full detail, no fps cap, blur on). For strong GPUs.
    Bonkers,
}

impl GpuLevel {
    /// Every level, for the settings picker.
    pub const ALL: [GpuLevel; 4] = [
        GpuLevel::Lite,
        GpuLevel::Moderate,
        GpuLevel::High,
        GpuLevel::Bonkers,
    ];
    /// The lowercase name used in the config and the picker.
    pub fn name(self) -> &'static str {
        match self {
            GpuLevel::Lite => "lite",
            GpuLevel::Moderate => "moderate",
            GpuLevel::High => "high",
            GpuLevel::Bonkers => "bonkers",
        }
    }
    /// fbm octave count the shaders run (the dominant per-pixel cost). `High`/`Bonkers`
    /// keep the original 5 → look unchanged; lower tiers soften detail to save work.
    pub fn fbm_octaves(self) -> f32 {
        match self {
            GpuLevel::Lite => 2.0,
            GpuLevel::Moderate => 3.0,
            GpuLevel::High | GpuLevel::Bonkers => 5.0,
        }
    }
    /// Target frame-rate cap (the redraw loop honors it). `None` = uncapped (vsync).
    pub fn fps_cap(self) -> Option<u32> {
        match self {
            GpuLevel::Lite => Some(30),
            GpuLevel::Moderate | GpuLevel::High => Some(60),
            GpuLevel::Bonkers => None,
        }
    }
    /// Whether the second full-screen frosted-card blur pass is allowed at this tier.
    /// (The user's `card_blur` toggle still has to be on; this only gates it off low.)
    pub fn allows_blur(self) -> bool {
        matches!(self, GpuLevel::High | GpuLevel::Bonkers)
    }
}

impl std::fmt::Display for GpuLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Where the login card sits on screen. The greeter maps this to container alignment;
/// the card keeps its width and the rest of the screen shows the sky/wallpaper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardPos {
    #[default]
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl CardPos {
    /// Every placement, for the settings picker.
    pub const ALL: [CardPos; 5] = [
        CardPos::Center,
        CardPos::Left,
        CardPos::Right,
        CardPos::Top,
        CardPos::Bottom,
    ];
    /// The lowercase name used in the config and the picker.
    pub fn name(self) -> &'static str {
        match self {
            CardPos::Center => "center",
            CardPos::Left => "left",
            CardPos::Right => "right",
            CardPos::Top => "top",
            CardPos::Bottom => "bottom",
        }
    }
}

impl std::fmt::Display for CardPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// The card's loading-emblem style (when no `logo` image is set). The comet is door's
/// signature; the rest are classic spinners. New styles are a variant + a branch in
/// the spinner shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpinnerStyle {
    #[default]
    Comet,
    Ring,
    Dots,
    Pulse,
    /// No emblem at all — a clean card with no spinner.
    None,
}

impl SpinnerStyle {
    /// Every style, for the settings picker.
    pub const ALL: [SpinnerStyle; 5] = [
        SpinnerStyle::Comet,
        SpinnerStyle::Ring,
        SpinnerStyle::Dots,
        SpinnerStyle::Pulse,
        SpinnerStyle::None,
    ];
    /// The shader selector value (unused for `None`, which draws no emblem).
    pub fn shader_id(self) -> f32 {
        match self {
            SpinnerStyle::Comet => 0.0,
            SpinnerStyle::Ring => 1.0,
            SpinnerStyle::Dots => 2.0,
            SpinnerStyle::Pulse => 3.0,
            SpinnerStyle::None => 0.0,
        }
    }
    /// Whether this style draws no emblem (so the card shows none).
    pub fn is_hidden(self) -> bool {
        matches!(self, SpinnerStyle::None)
    }
    /// The lowercase name used in the config and the picker.
    pub fn name(self) -> &'static str {
        match self {
            SpinnerStyle::Comet => "comet",
            SpinnerStyle::Ring => "ring",
            SpinnerStyle::Dots => "dots",
            SpinnerStyle::Pulse => "pulse",
            SpinnerStyle::None => "none",
        }
    }
}

impl std::fmt::Display for SpinnerStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// How the card renders the time: the default digital readout, or a drawn analog
/// clock face (a canvas widget — hour/minute/second hands over a ticked rim). Both
/// share `clock_size` for scale and `clock_24h`/`clock_seconds` for behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockStyle {
    #[default]
    Digital,
    Analog,
}

impl ClockStyle {
    /// Every style, for the settings picker.
    pub const ALL: [ClockStyle; 2] = [ClockStyle::Digital, ClockStyle::Analog];
    /// The lowercase name used in the config and the picker.
    pub fn name(self) -> &'static str {
        match self {
            ClockStyle::Digital => "digital",
            ClockStyle::Analog => "analog",
        }
    }
}

impl std::fmt::Display for ClockStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// The card text weight. Maps to an `iced::font::Weight`; applied as the greeter's
/// default font weight (so it needs the configured family to ship that weight).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontWeight {
    Light,
    #[default]
    Regular,
    Medium,
    Semibold,
    Bold,
}

impl FontWeight {
    /// Every weight, for the settings picker.
    pub const ALL: [FontWeight; 5] = [
        FontWeight::Light,
        FontWeight::Regular,
        FontWeight::Medium,
        FontWeight::Semibold,
        FontWeight::Bold,
    ];
    /// The lowercase name used in the config and the picker.
    pub fn name(self) -> &'static str {
        match self {
            FontWeight::Light => "light",
            FontWeight::Regular => "regular",
            FontWeight::Medium => "medium",
            FontWeight::Semibold => "semibold",
            FontWeight::Bold => "bold",
        }
    }
    /// The corresponding iced font weight.
    pub fn iced(self) -> iced::font::Weight {
        match self {
            FontWeight::Light => iced::font::Weight::Light,
            FontWeight::Regular => iced::font::Weight::Normal,
            FontWeight::Medium => iced::font::Weight::Medium,
            FontWeight::Semibold => iced::font::Weight::Semibold,
            FontWeight::Bold => iced::font::Weight::Bold,
        }
    }
}

impl std::fmt::Display for FontWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// The resolved theme the UI renders against. Every field has a built-in default.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Full-bleed background image; `None` (or a missing file) → solid `background`.
    pub wallpaper: Option<PathBuf>,
    /// Solid fill shown behind/instead of the wallpaper.
    pub background: Color,
    /// The frosted login card fill — translucent (low alpha) over the wallpaper.
    pub card: Color,
    /// Primary accent (focus, the sign-in button).
    pub accent: Color,
    /// Main text/field color.
    pub foreground: Color,
    /// Secondary text (status line, date, placeholders).
    pub muted: Color,
    /// Input field fill (slightly lifted from the card).
    pub field: Color,
    /// Optional logo image shown in the card.
    pub logo: Option<PathBuf>,
    /// Optional font family name (must be installed system-wide); `None` → stock.
    pub font: Option<String>,
    /// Card corner radius (px).
    pub corner_radius: f32,
    /// Card width (px).
    pub card_width: f32,
    /// Where the card sits on screen. Shared.
    pub card_pos: CardPos,
    /// Whether to show the clock + date in the card.
    pub show_clock: bool,
    /// Whether to run the animated sky (twinkling stars + drifting comet). Off → a
    /// still wallpaper, like the battery half of the Plasma comet wallpaper.
    pub animate: bool,
    /// True for the light (Tokyo Night Day) variant — the sky recolors for a light
    /// background. Set by which built-in this resolved from, not from the file.
    pub is_day: bool,
    /// Which sky scene to render. `Auto` = day/night by clock; others force a scene.
    /// Shared.
    pub sky_mode: SkyMode,
    /// Accessibility: when true, all motion is stilled at load (no sky animation,
    /// breathing, parallax, grain, fade, or spinner motion). Shared.
    pub reduced_motion: bool,
    /// Global GPU-budget level. Bundles render-cost levers (shader detail,
    /// fps cap, blur gating). Global, never per-scene/per-preset. Shared. `High` =
    /// current look unchanged.
    pub gpu_level: GpuLevel,
    /// Comet-spinner head-glow intensity (0–1). Per variant: bright glow blends on a
    /// dark card but bands on a light one, so day defaults low. 0 = crisp, no bloom.
    pub spinner_glow: f32,
    /// Comet-spinner rotation speed (rad/s). Shared across variants.
    pub spinner_speed: f32,
    /// The rotating comet's color (head + trail). Per variant.
    pub spinner_comet: Color,
    /// The static ring of dots the comet passes over. Per variant.
    pub spinner_track: Color,
    /// Trail length, 0.2–1.0 (shorter = crisper, fewer overlapping dots — helps on a
    /// light card; longer = a softer glowing ribbon, lovely on dark). Per variant.
    pub spinner_trail: f32,
    /// The drifting *background-sky* comet's color — distinct from `spinner_comet`
    /// (which is tuned for the card emblem; the sky comet sits on the sky). Per variant.
    pub comet_color: Color,

    // ── Sky controls ──
    /// The sky glow/haze tint — the night sky-glow color and the daytime sun/zenith
    /// tint. Per variant (indigo by night, sky-blue by day by default).
    pub glow_color: Color,
    /// Sky glow strength: the haze at night, the sun-halo by day (0–1+). Per variant.
    pub sky_glow: f32,
    /// Star field density, 0–1 (higher = more stars). Shared.
    pub star_density: f32,
    /// Star twinkle-speed multiplier (1 = default). Shared.
    pub star_twinkle: f32,
    /// Whether the drifting background comet runs. Shared.
    pub comet_enabled: bool,
    /// Seconds between background-comet sweeps (lower = more frequent). Shared.
    pub comet_interval: f32,
    /// Daytime cloud coverage multiplier (1 = default; 0 = clear sky). Shared.
    pub cloud_amount: f32,
    /// Daytime cloud drift-speed multiplier (1 = default). Shared.
    pub cloud_speed: f32,
    /// Daytime sun horizontal position, 0–1 (fraction of width; 0 = left edge).
    /// Shared (only the day sky draws a sun).
    pub sun_x: f32,
    /// Daytime sun vertical position, 0–1 (0 = top). Shared (day-only effect).
    pub sun_y: f32,
    /// Daytime sun halo radius (bigger = wider, softer glow). Shared (day-only).
    pub sun_size: f32,
    /// Daytime sun halo intensity, 0–1 (0 = no bloom). Shared (day-only effect).
    pub sun_intensity: f32,
    /// Daytime sun halo tint (warm by default). Shared (only the day sky has a sun).
    pub sun_color: Color,
    /// Night sky-glow center, 0–1 UV — x. Shared (only the night sky has this glow).
    pub glow_x: f32,
    /// Night sky-glow center, 0–1 UV — y. Shared.
    pub glow_y: f32,
    /// Daytime horizon-haze strength, 0–1. Shared (day-only effect).
    pub day_haze: f32,
    /// Number of night star layers (1–5; rounded). Shared.
    pub star_layers: f32,
    /// Star size multiplier (1 = default; bigger = larger stars). Shared.
    pub star_size: f32,
    /// Night nebula drift-speed multiplier (1 = default). Shared.
    pub nebula_speed: f32,
    /// Sky-comet path rotation in radians (0 = the default diagonal). Shared.
    pub comet_tilt: f32,
    /// Pause (seconds) after each sky-comet sweep. Shared.
    pub comet_pause: f32,
    /// Sky-comet width multiplier (1 = default). Shared.
    pub comet_width: f32,
    /// Daytime cloud sun-lit color. Shared (day-only effect).
    pub cloud_lit: Color,
    /// Daytime cloud shadowed-underside color. Shared (day-only effect).
    pub cloud_shadow: Color,
    /// Cursor-parallax strength for the starfield (0 = off; ~1 = subtle). Shared.
    pub cursor_parallax: f32,
    /// Night sky-glow breathing depth (0 = steady, the default; ~1 = a gentle pulse).
    /// Shared; cleared by reduced-motion.
    pub glow_pulse: f32,

    // ── Synthwave scene controls (M8 pilot; shared — only read when sky_mode=synthwave) ──
    /// Synthwave grid scroll speed (toward the viewer).
    pub synthwave_grid_speed: f32,
    /// Synthwave grid line density (higher = more lines).
    pub synthwave_grid_density: f32,
    /// Synthwave grid perspective spread (higher = wider toward the front).
    pub synthwave_grid_perspective: f32,
    /// Synthwave grid glow intensity.
    pub synthwave_grid_glow: f32,
    /// Synthwave sun radius (fraction of height).
    pub synthwave_sun_size: f32,
    /// Synthwave sun horizontal-stripe frequency.
    pub synthwave_sun_stripes: f32,
    /// Synthwave sun bloom strength.
    pub synthwave_sun_bloom: f32,
    /// Synthwave horizon height (0 = top … 1 = bottom).
    pub synthwave_horizon: f32,
    /// Synthwave neon grid colour.
    pub synthwave_grid_color: Color,
    /// Synthwave upper-sky colour.
    pub synthwave_sky_top: Color,
    /// Synthwave lower-sky / sun-base colour.
    pub synthwave_sky_bottom: Color,

    // ── Storm scene controls (M8; shared — read when sky_mode=storm) ──
    /// Storm lightning rate (strike windows per second).
    pub storm_lightning_rate: f32,
    /// Storm per-window strike chance (0–1).
    pub storm_strike_chance: f32,
    /// Storm cloud-tint density multiplier.
    pub storm_cloud_density: f32,
    /// Storm lightning-bolt colour.
    pub storm_bolt_color: Color,
    /// Storm whole-sky flash colour.
    pub storm_flash_color: Color,

    // ── Rain scene controls (M8) ──
    /// Rain fall speed (base; layers add to it).
    pub rain_fall_speed: f32,
    /// Rain density (0 = none … ~0.3 = heavy).
    pub rain_density: f32,
    /// Rain diagonal slant.
    pub rain_slant: f32,
    /// Rain streak brightness.
    pub rain_intensity: f32,
    /// Rain streak colour.
    pub rain_color: Color,

    // ── Snow scene controls (M8) ──
    /// Snow fall speed (base; layers add to it).
    pub snow_fall_speed: f32,
    /// Snow density (0 = none … ~0.3 = heavy).
    pub snow_density: f32,
    /// Snow horizontal sway amount.
    pub snow_sway: f32,
    /// Snow flake size (higher = smaller flakes).
    pub snow_flake_size: f32,
    /// Snow flake colour.
    pub snow_color: Color,

    // ── Fire scene controls (M8) ──
    /// Fire rise speed (flames scroll upward).
    pub fire_rise_speed: f32,
    /// Fire flame height (lower = taller flames).
    pub fire_flame_height: f32,
    /// Fire base flame colour.
    pub fire_flame_color: Color,
    /// Fire hot-tip colour.
    pub fire_tip_color: Color,

    // ── Aurora scene controls (M8) ──
    /// Aurora curtain drift speed.
    pub aurora_speed: f32,
    /// Aurora curtain drop-off (lower = taller curtains hanging down).
    pub aurora_drop: f32,
    /// Aurora vertical ray frequency (shimmer detail).
    pub aurora_ray_freq: f32,
    /// Aurora overall brightness.
    pub aurora_intensity: f32,
    /// Aurora green curtain colour.
    pub aurora_green: Color,
    /// Aurora magenta curtain colour.
    pub aurora_magenta: Color,

    // ── Plasma scene controls (M8) ──
    /// Plasma flow speed.
    pub plasma_speed: f32,
    /// Plasma pattern scale (higher = finer cells).
    pub plasma_scale: f32,
    /// Plasma colour saturation (0 = grey, 0.5 = full rainbow).
    pub plasma_saturation: f32,
    /// Plasma tint multiplier (white = the raw rainbow).
    pub plasma_tint: Color,

    // ── Water scene controls (M8) ──
    /// Water flow speed.
    pub water_speed: f32,
    /// Water ripple scale (higher = finer ripples).
    pub water_scale: f32,
    /// Water domain-warp strength (organic distortion).
    pub water_ripple: f32,
    /// Water caustic brightness.
    pub water_caustic: f32,
    /// Water caustic web colour.
    pub water_caustic_color: Color,
    /// Water deep (top) colour.
    pub water_deep: Color,
    /// Water shallow (bottom) colour.
    pub water_shallow: Color,
    /// Card vertical-gradient strength (0 = flat fill; ~1 = a lit top sheen). Shared.
    pub card_gradient: f32,
    /// Film-grain strength over the whole sky (0 = off). Shared.
    pub grain: f32,
    /// Vignette strength — screen-edge darkening (0 = off). Shared.
    pub vignette: f32,
    /// True backdrop blur behind the card (a frosted-glass sample of the sky). Most
    /// visible with a translucent card; costs a little GPU. Shared.
    pub card_blur: bool,

    // ── Spinner controls ──
    /// Card spinner size in px. Shared.
    pub spinner_size: f32,
    /// Spinner head pulse/breathing speed (1 = default). Shared.
    pub spinner_pulse: f32,
    /// Spinner comet orbit radius (fraction of the emblem; default 0.30). Shared.
    pub spinner_orbit: f32,
    /// The card loading-emblem style (comet/ring/dots/pulse). Shared.
    pub spinner_style: SpinnerStyle,

    // ── Card / behavior controls ──
    /// Card drop-shadow blur radius (px). Shared.
    pub card_shadow_blur: f32,
    /// Card drop-shadow opacity, 0–1. Shared.
    pub card_shadow_opacity: f32,
    /// Card accent-hairline breathing speed (1 = default). Shared.
    pub accent_breathing: f32,
    /// Input/button corner radius (px). Shared.
    pub field_radius: f32,
    /// Status-line color when a login fails (a warm red by default). Per variant.
    pub error_color: Color,
    /// Backdrop fill behind the logo / comet spinner — a rounded box. Transparent by
    /// default (invisible); set an alpha to show a tile behind the emblem. Per variant.
    pub logo_box: Color,
    /// Corner radius of the logo backdrop box (px). Shared.
    pub logo_box_radius: f32,
    /// 24-hour clock (`true`) vs 12-hour. Shared.
    pub clock_24h: bool,
    /// Show seconds on the clock (`HH:MM:SS`). Shared.
    pub clock_seconds: bool,
    /// Clock font size (px) — also the analog face scale. Shared.
    pub clock_size: f32,
    /// Digital readout vs a drawn analog clock face. Shared.
    pub clock_style: ClockStyle,
    /// Optional `strftime` format for the digital clock (e.g. `"%a %H:%M"`); `None`
    /// uses the built-in `clock_24h`/`clock_seconds` formatting. Shared.
    pub clock_format: Option<String>,
    /// Text size multiplier for the whole card (1 = default; >1 = large-text). Shared.
    pub font_scale: f32,
    /// Card text weight (applied as the greeter's default font weight). Shared.
    pub font_weight: FontWeight,
    /// Launch fade-in duration (ms). Shared.
    pub fade_ms: f32,

    // ── Expert (advanced) ──
    /// Night sky-glow falloff (higher = tighter halo). Shared.
    pub glow_falloff: f32,
    /// Night nebula cloud amount, 0–1. Shared.
    pub nebula_amount: f32,
    /// Background-comet tail fade rate (higher = shorter tail). Shared.
    pub comet_tail_decay: f32,
    /// Spinner orbit-ring intensity, 0–1. Shared.
    pub spinner_ring: f32,
}

impl Default for Theme {
    /// The built-in beautiful default: a dark, Tokyo-Night palette with a
    /// translucent card and a soft blue accent. Renders fully even with no assets.
    fn default() -> Self {
        Theme {
            wallpaper: None,
            background: Color::rgb(0x1a, 0x1b, 0x26),
            // A dark, glassy card — translucent so the wallpaper reads through.
            card: Color::rgba(0x16, 0x16, 0x1e, 0xd8),
            accent: Color::rgb(0x7a, 0xa2, 0xf7),
            foreground: Color::rgb(0xc0, 0xca, 0xf5),
            muted: Color::rgb(0x56, 0x5f, 0x89),
            field: Color::rgb(0x29, 0x2e, 0x42),
            logo: None,
            font: None,
            corner_radius: 16.0,
            card_width: 300.0,
            card_pos: CardPos::Center,
            show_clock: true,
            animate: true,
            is_day: false,
            sky_mode: SkyMode::Auto,
            reduced_motion: false,
            gpu_level: GpuLevel::High,
            spinner_glow: 1.0,
            spinner_speed: 2.5,
            spinner_comet: Color::rgb(0x7d, 0xcf, 0xff),
            spinner_track: Color::rgb(0x7a, 0xa2, 0xf7),
            spinner_trail: 1.0,
            comet_color: Color::rgb(0x7d, 0xcf, 0xff),
            glow_color: Color::rgb(0x3d, 0x59, 0xa1),
            sky_glow: 0.50,
            star_density: 0.47,
            star_twinkle: 1.0,
            comet_enabled: true,
            comet_interval: 9.5,
            cloud_amount: 1.0,
            cloud_speed: 1.0,
            sun_x: 0.24,
            sun_y: 0.18,
            sun_size: 0.25,
            sun_intensity: 0.38,
            sun_color: Color::rgb(0xff, 0xe8, 0xb0),
            glow_x: 0.5,
            glow_y: 0.44,
            day_haze: 0.25,
            star_layers: 3.0,
            star_size: 1.0,
            nebula_speed: 1.0,
            comet_tilt: 0.0,
            comet_pause: 2.5,
            comet_width: 1.0,
            cloud_lit: Color::rgb(0xff, 0xff, 0xff),
            cloud_shadow: Color::rgb(0xb4, 0xc2, 0xdb),
            cursor_parallax: 1.0,
            glow_pulse: 0.0,
            synthwave_grid_speed: 1.2,
            synthwave_grid_density: 0.55,
            synthwave_grid_perspective: 0.6,
            synthwave_grid_glow: 0.7,
            synthwave_sun_size: 0.22,
            synthwave_sun_stripes: 120.0,
            synthwave_sun_bloom: 0.25,
            synthwave_horizon: 0.56,
            synthwave_grid_color: Color::rgb(0x00, 0xe6, 0xff),
            synthwave_sky_top: Color::rgb(0x29, 0x0d, 0x47),
            synthwave_sky_bottom: Color::rgb(0xf2, 0x45, 0x8c),
            storm_lightning_rate: 0.8,
            storm_strike_chance: 0.32,
            storm_cloud_density: 1.0,
            storm_bolt_color: Color::rgb(0xd9, 0xe6, 0xff),
            storm_flash_color: Color::rgb(0x8c, 0x99, 0xcc),
            rain_fall_speed: 5.0,
            rain_density: 0.07,
            rain_slant: 4.0,
            rain_intensity: 0.5,
            rain_color: Color::rgb(0x8c, 0x9e, 0xb8),
            snow_fall_speed: 0.9,
            snow_density: 0.14,
            snow_sway: 0.6,
            snow_flake_size: 18.0,
            snow_color: Color::rgb(0xff, 0xff, 0xff),
            fire_rise_speed: 2.0,
            fire_flame_height: 0.22,
            fire_flame_color: Color::rgb(0xff, 0x4b, 0x0d),
            fire_tip_color: Color::rgb(0xff, 0xe6, 0x80),
            aurora_speed: 0.12,
            aurora_drop: 3.2,
            aurora_ray_freq: 50.0,
            aurora_intensity: 0.6,
            aurora_green: Color::rgb(0x2e, 0xff, 0xa8),
            aurora_magenta: Color::rgb(0x9e, 0x4d, 0xff),
            plasma_speed: 0.5,
            plasma_scale: 8.0,
            plasma_saturation: 0.5,
            plasma_tint: Color::rgb(0xff, 0xff, 0xff),
            water_speed: 0.6,
            water_scale: 5.0,
            water_ripple: 0.6,
            water_caustic: 0.85,
            water_caustic_color: Color::rgb(0x73, 0xf2, 0xff),
            water_deep: Color::rgb(0x00, 0x1f, 0x38),
            water_shallow: Color::rgb(0x00, 0x4d, 0x6b),
            card_gradient: 0.45,
            grain: 0.0,
            vignette: 0.2,
            card_blur: true,
            spinner_size: 52.0,
            spinner_pulse: 1.0,
            spinner_orbit: 0.30,
            spinner_style: SpinnerStyle::Comet,
            card_shadow_blur: 34.0,
            card_shadow_opacity: 0.45,
            accent_breathing: 1.0,
            field_radius: 10.0,
            error_color: Color::rgb(0xf7, 0x76, 0x8e),
            logo_box: Color::rgba(0x00, 0x00, 0x00, 0x00),
            logo_box_radius: 14.0,
            clock_24h: true,
            clock_seconds: false,
            clock_size: 56.0,
            clock_style: ClockStyle::Digital,
            clock_format: None,
            font_scale: 1.0,
            font_weight: FontWeight::Regular,
            fade_ms: 384.0,
            glow_falloff: 3.2,
            nebula_amount: 0.12,
            comet_tail_decay: 9.0,
            spinner_ring: 0.10,
        }
    }
}

impl Theme {
    /// The built-in **day** (Tokyo Night Day, light) palette — the light twin of
    /// [`Theme::default`]. Structural keys (font, sizes, behavior) are taken from
    /// the config at load time; colors and `is_day` come from here.
    pub fn day() -> Self {
        Theme {
            wallpaper: None,
            background: Color::rgb(0xe1, 0xe2, 0xe7),
            // Very translucent glass — the day sky reads strongly through it.
            card: Color::rgba(0xf4, 0xf6, 0xfb, 0x26),
            accent: Color::rgb(0x2e, 0x7d, 0xe9),
            foreground: Color::rgb(0x34, 0x3b, 0x58),
            muted: Color::rgb(0x54, 0x5c, 0x7e),
            field: Color::rgba(0xff, 0xff, 0xff, 0x99),
            logo: None,
            font: None,
            corner_radius: 16.0,
            card_width: 300.0,
            card_pos: CardPos::Center,
            show_clock: true,
            animate: true,
            is_day: true,
            sky_mode: SkyMode::Auto,
            reduced_motion: false,
            gpu_level: GpuLevel::High,
            // No head bloom on the bright card (the bloom bands on light); crisp.
            spinner_glow: 0.0,
            spinner_speed: 2.5,
            // Deep solid comet + a short trail so it stays crisp on the white card.
            spinner_comet: Color::rgb(0x21, 0x46, 0x93),
            spinner_track: Color::rgb(0x2e, 0x7d, 0xe9),
            spinner_trail: 0.55,
            // A brighter sky comet so it reads against the luminous day sky.
            comet_color: Color::rgb(0x6f, 0x9f, 0xe0),
            // Per-variant: a touch more sun-halo by day; a darker red on the light card.
            glow_color: Color::rgb(0x8f, 0xb6, 0xff),
            sky_glow: 0.55,
            error_color: Color::rgb(0xc0, 0x33, 0x4d),
            logo_box: Color::rgba(0x00, 0x00, 0x00, 0x00),
            // Shared keys: same defaults (the config's top-level values win at load).
            logo_box_radius: 14.0,
            star_density: 0.47,
            star_twinkle: 1.0,
            comet_enabled: true,
            comet_interval: 9.5,
            cloud_amount: 1.0,
            cloud_speed: 1.0,
            sun_x: 0.24,
            sun_y: 0.18,
            sun_size: 0.25,
            sun_intensity: 0.38,
            sun_color: Color::rgb(0xff, 0xe8, 0xb0),
            glow_x: 0.5,
            glow_y: 0.44,
            day_haze: 0.25,
            star_layers: 3.0,
            star_size: 1.0,
            nebula_speed: 1.0,
            comet_tilt: 0.0,
            comet_pause: 2.5,
            comet_width: 1.0,
            cloud_lit: Color::rgb(0xff, 0xff, 0xff),
            cloud_shadow: Color::rgb(0xb4, 0xc2, 0xdb),
            cursor_parallax: 1.0,
            glow_pulse: 0.0,
            synthwave_grid_speed: 1.2,
            synthwave_grid_density: 0.55,
            synthwave_grid_perspective: 0.6,
            synthwave_grid_glow: 0.7,
            synthwave_sun_size: 0.22,
            synthwave_sun_stripes: 120.0,
            synthwave_sun_bloom: 0.25,
            synthwave_horizon: 0.56,
            synthwave_grid_color: Color::rgb(0x00, 0xe6, 0xff),
            synthwave_sky_top: Color::rgb(0x29, 0x0d, 0x47),
            synthwave_sky_bottom: Color::rgb(0xf2, 0x45, 0x8c),
            storm_lightning_rate: 0.8,
            storm_strike_chance: 0.32,
            storm_cloud_density: 1.0,
            storm_bolt_color: Color::rgb(0xd9, 0xe6, 0xff),
            storm_flash_color: Color::rgb(0x8c, 0x99, 0xcc),
            rain_fall_speed: 5.0,
            rain_density: 0.07,
            rain_slant: 4.0,
            rain_intensity: 0.5,
            rain_color: Color::rgb(0x8c, 0x9e, 0xb8),
            snow_fall_speed: 0.9,
            snow_density: 0.14,
            snow_sway: 0.6,
            snow_flake_size: 18.0,
            snow_color: Color::rgb(0xff, 0xff, 0xff),
            fire_rise_speed: 2.0,
            fire_flame_height: 0.22,
            fire_flame_color: Color::rgb(0xff, 0x4b, 0x0d),
            fire_tip_color: Color::rgb(0xff, 0xe6, 0x80),
            aurora_speed: 0.12,
            aurora_drop: 3.2,
            aurora_ray_freq: 50.0,
            aurora_intensity: 0.6,
            aurora_green: Color::rgb(0x2e, 0xff, 0xa8),
            aurora_magenta: Color::rgb(0x9e, 0x4d, 0xff),
            plasma_speed: 0.5,
            plasma_scale: 8.0,
            plasma_saturation: 0.5,
            plasma_tint: Color::rgb(0xff, 0xff, 0xff),
            water_speed: 0.6,
            water_scale: 5.0,
            water_ripple: 0.6,
            water_caustic: 0.85,
            water_caustic_color: Color::rgb(0x73, 0xf2, 0xff),
            water_deep: Color::rgb(0x00, 0x1f, 0x38),
            water_shallow: Color::rgb(0x00, 0x4d, 0x6b),
            card_gradient: 0.45,
            grain: 0.0,
            vignette: 0.2,
            card_blur: true,
            spinner_size: 52.0,
            spinner_pulse: 1.0,
            spinner_orbit: 0.30,
            spinner_style: SpinnerStyle::Comet,
            card_shadow_blur: 34.0,
            card_shadow_opacity: 0.45,
            accent_breathing: 1.0,
            field_radius: 10.0,
            clock_24h: true,
            clock_seconds: false,
            clock_size: 56.0,
            clock_style: ClockStyle::Digital,
            clock_format: None,
            font_scale: 1.0,
            font_weight: FontWeight::Regular,
            fade_ms: 384.0,
            glow_falloff: 3.2,
            nebula_amount: 0.12,
            comet_tail_decay: 9.0,
            spinner_ring: 0.10,
        }
    }
}

/// The on-disk form: every field optional, merged over [`Theme::default`]. Strict
/// (`deny_unknown_fields`) so a typo'd key is a loud parse error, not silent stock.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    wallpaper: Option<String>,
    background: Option<String>,
    card: Option<String>,
    accent: Option<String>,
    foreground: Option<String>,
    muted: Option<String>,
    field: Option<String>,
    font: Option<String>,
    logo: Option<String>,
    corner_radius: Option<f32>,
    card_width: Option<f32>,
    card_pos: Option<CardPos>,
    show_clock: Option<bool>,
    animate: Option<bool>,
    sky_mode: Option<SkyMode>,
    reduced_motion: Option<bool>,
    gpu_level: Option<GpuLevel>,
    spinner_glow: Option<f32>,
    spinner_speed: Option<f32>,
    spinner_comet: Option<String>,
    spinner_track: Option<String>,
    spinner_trail: Option<f32>,
    comet_color: Option<String>,
    glow_color: Option<String>,
    sky_glow: Option<f32>,
    star_density: Option<f32>,
    star_twinkle: Option<f32>,
    comet_enabled: Option<bool>,
    comet_interval: Option<f32>,
    cloud_amount: Option<f32>,
    cloud_speed: Option<f32>,
    sun_x: Option<f32>,
    sun_y: Option<f32>,
    sun_size: Option<f32>,
    sun_intensity: Option<f32>,
    sun_color: Option<String>,
    glow_x: Option<f32>,
    glow_y: Option<f32>,
    day_haze: Option<f32>,
    star_layers: Option<f32>,
    star_size: Option<f32>,
    nebula_speed: Option<f32>,
    comet_tilt: Option<f32>,
    comet_pause: Option<f32>,
    comet_width: Option<f32>,
    cloud_lit: Option<String>,
    cloud_shadow: Option<String>,
    cursor_parallax: Option<f32>,
    glow_pulse: Option<f32>,
    synthwave_grid_speed: Option<f32>,
    synthwave_grid_density: Option<f32>,
    synthwave_grid_perspective: Option<f32>,
    synthwave_grid_glow: Option<f32>,
    synthwave_sun_size: Option<f32>,
    synthwave_sun_stripes: Option<f32>,
    synthwave_sun_bloom: Option<f32>,
    synthwave_horizon: Option<f32>,
    synthwave_grid_color: Option<String>,
    synthwave_sky_top: Option<String>,
    synthwave_sky_bottom: Option<String>,
    storm_lightning_rate: Option<f32>,
    storm_strike_chance: Option<f32>,
    storm_cloud_density: Option<f32>,
    storm_bolt_color: Option<String>,
    storm_flash_color: Option<String>,
    rain_fall_speed: Option<f32>,
    rain_density: Option<f32>,
    rain_slant: Option<f32>,
    rain_intensity: Option<f32>,
    rain_color: Option<String>,
    snow_fall_speed: Option<f32>,
    snow_density: Option<f32>,
    snow_sway: Option<f32>,
    snow_flake_size: Option<f32>,
    snow_color: Option<String>,
    fire_rise_speed: Option<f32>,
    fire_flame_height: Option<f32>,
    fire_flame_color: Option<String>,
    fire_tip_color: Option<String>,
    aurora_speed: Option<f32>,
    aurora_drop: Option<f32>,
    aurora_ray_freq: Option<f32>,
    aurora_intensity: Option<f32>,
    aurora_green: Option<String>,
    aurora_magenta: Option<String>,
    plasma_speed: Option<f32>,
    plasma_scale: Option<f32>,
    plasma_saturation: Option<f32>,
    plasma_tint: Option<String>,
    water_speed: Option<f32>,
    water_scale: Option<f32>,
    water_ripple: Option<f32>,
    water_caustic: Option<f32>,
    water_caustic_color: Option<String>,
    water_deep: Option<String>,
    water_shallow: Option<String>,
    card_gradient: Option<f32>,
    grain: Option<f32>,
    vignette: Option<f32>,
    card_blur: Option<bool>,
    spinner_orbit: Option<f32>,
    spinner_style: Option<SpinnerStyle>,
    spinner_size: Option<f32>,
    spinner_pulse: Option<f32>,
    card_shadow_blur: Option<f32>,
    card_shadow_opacity: Option<f32>,
    accent_breathing: Option<f32>,
    field_radius: Option<f32>,
    error_color: Option<String>,
    logo_box: Option<String>,
    logo_box_radius: Option<f32>,
    clock_24h: Option<bool>,
    clock_seconds: Option<bool>,
    clock_size: Option<f32>,
    clock_style: Option<ClockStyle>,
    clock_format: Option<String>,
    font_scale: Option<f32>,
    font_weight: Option<FontWeight>,
    fade_ms: Option<f32>,
    glow_falloff: Option<f32>,
    nebula_amount: Option<f32>,
    comet_tail_decay: Option<f32>,
    spinner_ring: Option<f32>,
    /// Local day window for the greeter's auto day/night, `"HH:MM"` (default
    /// 07:00–19:00). Inside the window the greeter uses the day palette.
    day_start: Option<String>,
    day_end: Option<String>,
    /// `[day]` table — color/wallpaper/logo overrides for the day variant (over the
    /// built-in Tokyo Night Day palette). Structural keys are shared from top level.
    day: Option<DayFile>,
}

/// The `[day]` override table: the visual (per-variant) keys only.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct DayFile {
    wallpaper: Option<String>,
    background: Option<String>,
    card: Option<String>,
    accent: Option<String>,
    foreground: Option<String>,
    muted: Option<String>,
    field: Option<String>,
    logo: Option<String>,
    spinner_glow: Option<f32>,
    spinner_comet: Option<String>,
    spinner_track: Option<String>,
    spinner_trail: Option<f32>,
    comet_color: Option<String>,
    // Per-variant sky/behavior overrides.
    glow_color: Option<String>,
    sky_glow: Option<f32>,
    error_color: Option<String>,
    logo_box: Option<String>,
}

/// Overwrite `slot` with `v` if the file provided one (the merge idiom for every
/// scalar/bool key — keeps the merge functions terse as the schema grows).
fn merge_f32(slot: &mut f32, v: Option<f32>) {
    if let Some(x) = v {
        *slot = x;
    }
}
fn merge_bool(slot: &mut bool, v: Option<bool>) {
    if let Some(x) = v {
        *slot = x;
    }
}

impl Theme {
    /// Resolve the theme from the first readable config path, merged over the
    /// default. Never fails — a missing/invalid file leaves the default standing
    /// (with a stderr note), because the greeter must always render.
    pub fn load() -> Theme {
        match Self::config_source() {
            Some((path, contents)) => match toml::from_str::<ThemeFile>(&contents) {
                Ok(file) => Theme::default().merged(file),
                Err(e) => {
                    eprintln!("door: ignoring malformed theme {}: {e}", path.display());
                    Theme::default()
                }
            },
            None => Theme::default(),
        }
    }

    /// First readable config (env override, then the system paths) as (path, text).
    fn config_source() -> Option<(PathBuf, String)> {
        let candidates = std::env::var_os(ENV_CONFIG)
            .map(PathBuf::from)
            .into_iter()
            .chain(SYSTEM_CONFIG_PATHS.iter().map(PathBuf::from));
        for path in candidates {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                return Some((path, contents));
            }
        }
        None
    }

    /// Overlay a parsed file onto this theme. A color that fails to parse keeps the
    /// existing value (with a warning) rather than discarding the whole config.
    fn merged(mut self, file: ThemeFile) -> Theme {
        let color = |field: &str, raw: Option<String>, current: Color| -> Color {
            match raw {
                Some(s) => Color::parse(&s).unwrap_or_else(|| {
                    eprintln!("door: theme '{field}' is not #rrggbb[aa]: {s:?}; keeping default");
                    current
                }),
                None => current,
            }
        };
        self.background = color("background", file.background, self.background);
        self.card = color("card", file.card, self.card);
        self.accent = color("accent", file.accent, self.accent);
        self.foreground = color("foreground", file.foreground, self.foreground);
        self.muted = color("muted", file.muted, self.muted);
        self.field = color("field", file.field, self.field);
        if let Some(w) = file.wallpaper {
            self.wallpaper = Some(PathBuf::from(w));
        }
        if let Some(l) = file.logo {
            self.logo = Some(PathBuf::from(l));
        }
        if file.font.is_some() {
            self.font = file.font;
        }
        if let Some(r) = file.corner_radius {
            self.corner_radius = r;
        }
        if let Some(w) = file.card_width {
            self.card_width = w;
        }
        if let Some(pos) = file.card_pos {
            self.card_pos = pos;
        }
        if let Some(c) = file.show_clock {
            self.show_clock = c;
        }
        if let Some(a) = file.animate {
            self.animate = a;
        }
        if let Some(m) = file.sky_mode {
            self.sky_mode = m;
        }
        merge_bool(&mut self.reduced_motion, file.reduced_motion);
        if let Some(g) = file.gpu_level {
            self.gpu_level = g;
        }
        if let Some(g) = file.spinner_glow {
            self.spinner_glow = g;
        }
        if let Some(s) = file.spinner_speed {
            self.spinner_speed = s;
        }
        self.spinner_comet = color("spinner_comet", file.spinner_comet, self.spinner_comet);
        self.spinner_track = color("spinner_track", file.spinner_track, self.spinner_track);
        if let Some(tr) = file.spinner_trail {
            self.spinner_trail = tr;
        }
        self.comet_color = color("comet_color", file.comet_color, self.comet_color);
        // New control surface (night values from the top level).
        self.glow_color = color("glow_color", file.glow_color, self.glow_color);
        merge_f32(&mut self.sky_glow, file.sky_glow);
        merge_f32(&mut self.star_density, file.star_density);
        merge_f32(&mut self.star_twinkle, file.star_twinkle);
        merge_bool(&mut self.comet_enabled, file.comet_enabled);
        merge_f32(&mut self.comet_interval, file.comet_interval);
        merge_f32(&mut self.cloud_amount, file.cloud_amount);
        merge_f32(&mut self.cloud_speed, file.cloud_speed);
        merge_f32(&mut self.sun_x, file.sun_x);
        merge_f32(&mut self.sun_y, file.sun_y);
        merge_f32(&mut self.sun_size, file.sun_size);
        merge_f32(&mut self.sun_intensity, file.sun_intensity);
        self.sun_color = color("sun_color", file.sun_color, self.sun_color);
        merge_f32(&mut self.glow_x, file.glow_x);
        merge_f32(&mut self.glow_y, file.glow_y);
        merge_f32(&mut self.day_haze, file.day_haze);
        merge_f32(&mut self.star_layers, file.star_layers);
        merge_f32(&mut self.star_size, file.star_size);
        merge_f32(&mut self.nebula_speed, file.nebula_speed);
        merge_f32(&mut self.comet_tilt, file.comet_tilt);
        merge_f32(&mut self.comet_pause, file.comet_pause);
        merge_f32(&mut self.comet_width, file.comet_width);
        self.cloud_lit = color("cloud_lit", file.cloud_lit, self.cloud_lit);
        self.cloud_shadow = color("cloud_shadow", file.cloud_shadow, self.cloud_shadow);
        merge_f32(&mut self.cursor_parallax, file.cursor_parallax);
        merge_f32(&mut self.glow_pulse, file.glow_pulse);
        merge_f32(&mut self.synthwave_grid_speed, file.synthwave_grid_speed);
        merge_f32(&mut self.synthwave_grid_density, file.synthwave_grid_density);
        merge_f32(&mut self.synthwave_grid_perspective, file.synthwave_grid_perspective);
        merge_f32(&mut self.synthwave_grid_glow, file.synthwave_grid_glow);
        merge_f32(&mut self.synthwave_sun_size, file.synthwave_sun_size);
        merge_f32(&mut self.synthwave_sun_stripes, file.synthwave_sun_stripes);
        merge_f32(&mut self.synthwave_sun_bloom, file.synthwave_sun_bloom);
        merge_f32(&mut self.synthwave_horizon, file.synthwave_horizon);
        self.synthwave_grid_color =
            color("synthwave_grid_color", file.synthwave_grid_color, self.synthwave_grid_color);
        self.synthwave_sky_top =
            color("synthwave_sky_top", file.synthwave_sky_top, self.synthwave_sky_top);
        self.synthwave_sky_bottom =
            color("synthwave_sky_bottom", file.synthwave_sky_bottom, self.synthwave_sky_bottom);
        merge_f32(&mut self.storm_lightning_rate, file.storm_lightning_rate);
        merge_f32(&mut self.storm_strike_chance, file.storm_strike_chance);
        merge_f32(&mut self.storm_cloud_density, file.storm_cloud_density);
        self.storm_bolt_color =
            color("storm_bolt_color", file.storm_bolt_color, self.storm_bolt_color);
        self.storm_flash_color =
            color("storm_flash_color", file.storm_flash_color, self.storm_flash_color);
        merge_f32(&mut self.rain_fall_speed, file.rain_fall_speed);
        merge_f32(&mut self.rain_density, file.rain_density);
        merge_f32(&mut self.rain_slant, file.rain_slant);
        merge_f32(&mut self.rain_intensity, file.rain_intensity);
        self.rain_color = color("rain_color", file.rain_color, self.rain_color);
        merge_f32(&mut self.snow_fall_speed, file.snow_fall_speed);
        merge_f32(&mut self.snow_density, file.snow_density);
        merge_f32(&mut self.snow_sway, file.snow_sway);
        merge_f32(&mut self.snow_flake_size, file.snow_flake_size);
        self.snow_color = color("snow_color", file.snow_color, self.snow_color);
        merge_f32(&mut self.fire_rise_speed, file.fire_rise_speed);
        merge_f32(&mut self.fire_flame_height, file.fire_flame_height);
        self.fire_flame_color =
            color("fire_flame_color", file.fire_flame_color, self.fire_flame_color);
        self.fire_tip_color = color("fire_tip_color", file.fire_tip_color, self.fire_tip_color);
        merge_f32(&mut self.aurora_speed, file.aurora_speed);
        merge_f32(&mut self.aurora_drop, file.aurora_drop);
        merge_f32(&mut self.aurora_ray_freq, file.aurora_ray_freq);
        merge_f32(&mut self.aurora_intensity, file.aurora_intensity);
        self.aurora_green = color("aurora_green", file.aurora_green, self.aurora_green);
        self.aurora_magenta = color("aurora_magenta", file.aurora_magenta, self.aurora_magenta);
        merge_f32(&mut self.plasma_speed, file.plasma_speed);
        merge_f32(&mut self.plasma_scale, file.plasma_scale);
        merge_f32(&mut self.plasma_saturation, file.plasma_saturation);
        self.plasma_tint = color("plasma_tint", file.plasma_tint, self.plasma_tint);
        merge_f32(&mut self.water_speed, file.water_speed);
        merge_f32(&mut self.water_scale, file.water_scale);
        merge_f32(&mut self.water_ripple, file.water_ripple);
        merge_f32(&mut self.water_caustic, file.water_caustic);
        self.water_caustic_color =
            color("water_caustic_color", file.water_caustic_color, self.water_caustic_color);
        self.water_deep = color("water_deep", file.water_deep, self.water_deep);
        self.water_shallow = color("water_shallow", file.water_shallow, self.water_shallow);
        merge_f32(&mut self.card_gradient, file.card_gradient);
        merge_f32(&mut self.grain, file.grain);
        merge_f32(&mut self.vignette, file.vignette);
        merge_bool(&mut self.card_blur, file.card_blur);
        merge_f32(&mut self.spinner_orbit, file.spinner_orbit);
        if let Some(s) = file.spinner_style {
            self.spinner_style = s;
        }
        merge_f32(&mut self.spinner_size, file.spinner_size);
        merge_f32(&mut self.spinner_pulse, file.spinner_pulse);
        merge_f32(&mut self.card_shadow_blur, file.card_shadow_blur);
        merge_f32(&mut self.card_shadow_opacity, file.card_shadow_opacity);
        merge_f32(&mut self.accent_breathing, file.accent_breathing);
        merge_f32(&mut self.field_radius, file.field_radius);
        self.error_color = color("error_color", file.error_color, self.error_color);
        self.logo_box = color("logo_box", file.logo_box, self.logo_box);
        merge_f32(&mut self.logo_box_radius, file.logo_box_radius);
        merge_bool(&mut self.clock_24h, file.clock_24h);
        merge_bool(&mut self.clock_seconds, file.clock_seconds);
        merge_f32(&mut self.clock_size, file.clock_size);
        if let Some(s) = file.clock_style {
            self.clock_style = s;
        }
        if file.clock_format.is_some() {
            self.clock_format = file.clock_format.clone();
        }
        merge_f32(&mut self.font_scale, file.font_scale);
        if let Some(w) = file.font_weight {
            self.font_weight = w;
        }
        merge_f32(&mut self.fade_ms, file.fade_ms);
        merge_f32(&mut self.glow_falloff, file.glow_falloff);
        merge_f32(&mut self.nebula_amount, file.nebula_amount);
        merge_f32(&mut self.comet_tail_decay, file.comet_tail_decay);
        merge_f32(&mut self.spinner_ring, file.spinner_ring);
        self
    }

    /// Apply only the *structural* keys (font, sizes, behavior) from a file —
    /// shared across day/night. Colors and wallpaper are left to the variant.
    fn merged_structural(mut self, file: &ThemeFile) -> Theme {
        if file.font.is_some() {
            self.font = file.font.clone();
        }
        if let Some(r) = file.corner_radius {
            self.corner_radius = r;
        }
        if let Some(w) = file.card_width {
            self.card_width = w;
        }
        if let Some(pos) = file.card_pos {
            self.card_pos = pos;
        }
        if let Some(c) = file.show_clock {
            self.show_clock = c;
        }
        if let Some(a) = file.animate {
            self.animate = a;
        }
        if let Some(m) = file.sky_mode {
            self.sky_mode = m;
        }
        merge_bool(&mut self.reduced_motion, file.reduced_motion);
        if let Some(g) = file.gpu_level {
            self.gpu_level = g;
        }
        // spinner_speed is shared across variants; spinner_glow is per-variant ([day]).
        if let Some(s) = file.spinner_speed {
            self.spinner_speed = s;
        }
        // The shared half of the new control surface (the day variant inherits these
        // from the top level; the per-variant keys come from [day] in `merged_day`).
        merge_f32(&mut self.star_density, file.star_density);
        merge_f32(&mut self.star_twinkle, file.star_twinkle);
        merge_bool(&mut self.comet_enabled, file.comet_enabled);
        merge_f32(&mut self.comet_interval, file.comet_interval);
        merge_f32(&mut self.cloud_amount, file.cloud_amount);
        merge_f32(&mut self.cloud_speed, file.cloud_speed);
        merge_f32(&mut self.sun_x, file.sun_x);
        merge_f32(&mut self.sun_y, file.sun_y);
        merge_f32(&mut self.sun_size, file.sun_size);
        merge_f32(&mut self.sun_intensity, file.sun_intensity);
        // sun_color is a shared color (only the day sky uses it); parse inline since
        // merged_structural has no color closure.
        if let Some(s) = &file.sun_color {
            if let Some(col) = Color::parse(s) {
                self.sun_color = col;
            }
        }
        merge_f32(&mut self.glow_x, file.glow_x);
        merge_f32(&mut self.glow_y, file.glow_y);
        merge_f32(&mut self.day_haze, file.day_haze);
        merge_f32(&mut self.star_layers, file.star_layers);
        merge_f32(&mut self.star_size, file.star_size);
        merge_f32(&mut self.nebula_speed, file.nebula_speed);
        merge_f32(&mut self.comet_tilt, file.comet_tilt);
        merge_f32(&mut self.comet_pause, file.comet_pause);
        merge_f32(&mut self.comet_width, file.comet_width);
        if let Some(s) = &file.cloud_lit {
            if let Some(col) = Color::parse(s) {
                self.cloud_lit = col;
            }
        }
        if let Some(s) = &file.cloud_shadow {
            if let Some(col) = Color::parse(s) {
                self.cloud_shadow = col;
            }
        }
        merge_f32(&mut self.cursor_parallax, file.cursor_parallax);
        merge_f32(&mut self.glow_pulse, file.glow_pulse);
        merge_f32(&mut self.synthwave_grid_speed, file.synthwave_grid_speed);
        merge_f32(&mut self.synthwave_grid_density, file.synthwave_grid_density);
        merge_f32(&mut self.synthwave_grid_perspective, file.synthwave_grid_perspective);
        merge_f32(&mut self.synthwave_grid_glow, file.synthwave_grid_glow);
        merge_f32(&mut self.synthwave_sun_size, file.synthwave_sun_size);
        merge_f32(&mut self.synthwave_sun_stripes, file.synthwave_sun_stripes);
        merge_f32(&mut self.synthwave_sun_bloom, file.synthwave_sun_bloom);
        merge_f32(&mut self.synthwave_horizon, file.synthwave_horizon);
        merge_f32(&mut self.storm_lightning_rate, file.storm_lightning_rate);
        merge_f32(&mut self.storm_strike_chance, file.storm_strike_chance);
        merge_f32(&mut self.storm_cloud_density, file.storm_cloud_density);
        merge_f32(&mut self.rain_fall_speed, file.rain_fall_speed);
        merge_f32(&mut self.rain_density, file.rain_density);
        merge_f32(&mut self.rain_slant, file.rain_slant);
        merge_f32(&mut self.rain_intensity, file.rain_intensity);
        merge_f32(&mut self.snow_fall_speed, file.snow_fall_speed);
        merge_f32(&mut self.snow_density, file.snow_density);
        merge_f32(&mut self.snow_sway, file.snow_sway);
        merge_f32(&mut self.snow_flake_size, file.snow_flake_size);
        merge_f32(&mut self.fire_rise_speed, file.fire_rise_speed);
        merge_f32(&mut self.fire_flame_height, file.fire_flame_height);
        merge_f32(&mut self.aurora_speed, file.aurora_speed);
        merge_f32(&mut self.aurora_drop, file.aurora_drop);
        merge_f32(&mut self.aurora_ray_freq, file.aurora_ray_freq);
        merge_f32(&mut self.aurora_intensity, file.aurora_intensity);
        merge_f32(&mut self.plasma_speed, file.plasma_speed);
        merge_f32(&mut self.plasma_scale, file.plasma_scale);
        merge_f32(&mut self.plasma_saturation, file.plasma_saturation);
        merge_f32(&mut self.water_speed, file.water_speed);
        merge_f32(&mut self.water_scale, file.water_scale);
        merge_f32(&mut self.water_ripple, file.water_ripple);
        merge_f32(&mut self.water_caustic, file.water_caustic);
        for (slot, s) in [
            (&mut self.synthwave_grid_color, &file.synthwave_grid_color),
            (&mut self.synthwave_sky_top, &file.synthwave_sky_top),
            (&mut self.synthwave_sky_bottom, &file.synthwave_sky_bottom),
            (&mut self.storm_bolt_color, &file.storm_bolt_color),
            (&mut self.storm_flash_color, &file.storm_flash_color),
            (&mut self.rain_color, &file.rain_color),
            (&mut self.snow_color, &file.snow_color),
            (&mut self.fire_flame_color, &file.fire_flame_color),
            (&mut self.fire_tip_color, &file.fire_tip_color),
            (&mut self.aurora_green, &file.aurora_green),
            (&mut self.aurora_magenta, &file.aurora_magenta),
            (&mut self.plasma_tint, &file.plasma_tint),
            (&mut self.water_caustic_color, &file.water_caustic_color),
            (&mut self.water_deep, &file.water_deep),
            (&mut self.water_shallow, &file.water_shallow),
        ] {
            if let Some(s) = s {
                if let Some(col) = Color::parse(s) {
                    *slot = col;
                }
            }
        }
        merge_f32(&mut self.card_gradient, file.card_gradient);
        merge_f32(&mut self.grain, file.grain);
        merge_f32(&mut self.vignette, file.vignette);
        merge_bool(&mut self.card_blur, file.card_blur);
        merge_f32(&mut self.spinner_orbit, file.spinner_orbit);
        if let Some(s) = file.spinner_style {
            self.spinner_style = s;
        }
        merge_f32(&mut self.spinner_size, file.spinner_size);
        merge_f32(&mut self.spinner_pulse, file.spinner_pulse);
        merge_f32(&mut self.card_shadow_blur, file.card_shadow_blur);
        merge_f32(&mut self.card_shadow_opacity, file.card_shadow_opacity);
        merge_f32(&mut self.accent_breathing, file.accent_breathing);
        merge_f32(&mut self.field_radius, file.field_radius);
        merge_f32(&mut self.logo_box_radius, file.logo_box_radius);
        merge_bool(&mut self.clock_24h, file.clock_24h);
        merge_bool(&mut self.clock_seconds, file.clock_seconds);
        merge_f32(&mut self.clock_size, file.clock_size);
        if let Some(s) = file.clock_style {
            self.clock_style = s;
        }
        if file.clock_format.is_some() {
            self.clock_format = file.clock_format.clone();
        }
        merge_f32(&mut self.font_scale, file.font_scale);
        if let Some(w) = file.font_weight {
            self.font_weight = w;
        }
        merge_f32(&mut self.fade_ms, file.fade_ms);
        merge_f32(&mut self.glow_falloff, file.glow_falloff);
        merge_f32(&mut self.nebula_amount, file.nebula_amount);
        merge_f32(&mut self.comet_tail_decay, file.comet_tail_decay);
        merge_f32(&mut self.spinner_ring, file.spinner_ring);
        self
    }

    /// Resolve the theme for a given local time (`now_minutes` = hour*60+min),
    /// auto-selecting the day or night variant by the configured window. The
    /// greeter calls this; it cannot read the user's color scheme (it runs before
    /// login), so the clock is the trigger. Night = the config's top-level palette;
    /// day = the built-in light palette with the config's structural keys.
    pub fn load_at(now_minutes: u32, month: u32) -> Theme {
        let (file, have) = match Self::config_source() {
            Some((path, contents)) => match toml::from_str::<ThemeFile>(&contents) {
                Ok(file) => (file, true),
                Err(e) => {
                    eprintln!("door: ignoring malformed theme {}: {e}", path.display());
                    (ThemeFile::default(), false)
                }
            },
            None => (ThemeFile::default(), false),
        };
        // Resolve the scene selector (Seasonal → a concrete scene by month) up front;
        // the caller owns the clock, so door-theme stays calendar-free.
        let mode = file.sky_mode.unwrap_or_default().resolved(month);
        let (start, end) = day_window(&file);
        // An explicit scene pins its card palette (night); auto/seasonal follow the clock.
        let use_day = match mode.prefers_day() {
            Some(day) => day,
            None => in_window(now_minutes, start, end),
        };
        let mut theme = if use_day {
            Theme::day().merged_structural(&file).merged_day(file.day)
        } else if have {
            Theme::default().merged(file)
        } else {
            Theme::default()
        };
        theme.sky_mode = mode;
        theme.apply_reduced_motion();
        theme
    }

    /// Accessibility: still all motion. A no-op unless `reduced_motion` is set; called
    /// after load so the config keeps its real values but the rendered theme is calm.
    pub fn apply_reduced_motion(&mut self) {
        if !self.reduced_motion {
            return;
        }
        self.animate = false;
        self.accent_breathing = 0.0;
        self.grain = 0.0;
        self.cursor_parallax = 0.0;
        self.glow_pulse = 0.0;
        self.fade_ms = 0.0;
        self.spinner_speed = 0.0;
        self.spinner_pulse = 0.0;
        self.star_twinkle = 0.0;
        self.comet_enabled = false;
    }

    /// Apply the `[day]` color/wallpaper/logo overrides over the built-in day palette.
    fn merged_day(mut self, day: Option<DayFile>) -> Theme {
        let Some(d) = day else {
            return self;
        };
        let color = |field: &str, raw: Option<String>, current: Color| match raw {
            Some(s) => Color::parse(&s).unwrap_or_else(|| {
                eprintln!("door: [day].{field} is not #rrggbb[aa]: {s:?}; keeping default");
                current
            }),
            None => current,
        };
        self.background = color("background", d.background, self.background);
        self.card = color("card", d.card, self.card);
        self.accent = color("accent", d.accent, self.accent);
        self.foreground = color("foreground", d.foreground, self.foreground);
        self.muted = color("muted", d.muted, self.muted);
        self.field = color("field", d.field, self.field);
        if let Some(w) = d.wallpaper {
            self.wallpaper = Some(PathBuf::from(w));
        }
        if let Some(l) = d.logo {
            self.logo = Some(PathBuf::from(l));
        }
        if let Some(g) = d.spinner_glow {
            self.spinner_glow = g;
        }
        self.spinner_comet = color("spinner_comet", d.spinner_comet, self.spinner_comet);
        self.spinner_track = color("spinner_track", d.spinner_track, self.spinner_track);
        if let Some(tr) = d.spinner_trail {
            self.spinner_trail = tr;
        }
        self.comet_color = color("comet_color", d.comet_color, self.comet_color);
        self.glow_color = color("glow_color", d.glow_color, self.glow_color);
        merge_f32(&mut self.sky_glow, d.sky_glow);
        self.error_color = color("error_color", d.error_color, self.error_color);
        self.logo_box = color("logo_box", d.logo_box, self.logo_box);
        self
    }

    /// Load both variants + the day window — for `door-settings`, which edits both.
    pub fn load_pair() -> (Theme, Theme, (u32, u32)) {
        let file = match Self::config_source() {
            Some((path, contents)) => toml::from_str::<ThemeFile>(&contents).unwrap_or_else(|e| {
                eprintln!("door: ignoring malformed theme {}: {e}", path.display());
                ThemeFile::default()
            }),
            None => ThemeFile::default(),
        };
        let window = day_window(&file);
        let night = Theme::default().merged(file.clone());
        let day = Theme::day()
            .merged_structural(&file)
            .merged_day(file.day.clone());
        (night, day, window)
    }

    /// Parse both variants + the day window from a config string (a preset file), the
    /// same way `load_pair` resolves the live config. `Err` on a malformed file so the
    /// caller can surface it (a preset the user explicitly picked shouldn't fail silent).
    pub fn parse_pair(contents: &str) -> Result<(Theme, Theme, (u32, u32)), String> {
        let file = toml::from_str::<ThemeFile>(contents).map_err(|e| e.to_string())?;
        let window = day_window(&file);
        let night = Theme::default().merged(file.clone());
        let day = Theme::day()
            .merged_structural(&file)
            .merged_day(file.day.clone());
        Ok((night, day, window))
    }

    /// Render a full `greeter.toml` with both palettes — the night top-level, the
    /// day window, and a `[day]` table. What `door-settings` saves.
    pub fn render_pair(night: &Theme, day: &Theme, day_start: &str, day_end: &str) -> String {
        let mut out = night.to_config_string();
        out.push_str(&format!("day_start  = {day_start:?}\n"));
        out.push_str(&format!("day_end    = {day_end:?}\n"));
        out.push_str("\n# Day variant overrides (light). Structural keys (font, sizes,\n");
        out.push_str("# behaviour) are shared from above; only colors/assets differ.\n");
        out.push_str("[day]\n");
        out.push_str(&format!("background  = {:?}\n", day.background.to_hex()));
        out.push_str(&format!("card        = {:?}\n", day.card.to_hex()));
        out.push_str(&format!("field       = {:?}\n", day.field.to_hex()));
        out.push_str(&format!("accent      = {:?}\n", day.accent.to_hex()));
        out.push_str(&format!("foreground  = {:?}\n", day.foreground.to_hex()));
        out.push_str(&format!("muted       = {:?}\n", day.muted.to_hex()));
        match &day.wallpaper {
            Some(w) => out.push_str(&format!("wallpaper = {:?}\n", w.display().to_string())),
            None => out.push_str("# wallpaper =\n"),
        }
        match &day.logo {
            Some(l) => out.push_str(&format!("logo = {:?}\n", l.display().to_string())),
            None => out.push_str("# logo =\n"),
        }
        out.push_str(&format!("spinner_glow = {}\n", day.spinner_glow));
        out.push_str(&format!(
            "spinner_comet = {:?}\n",
            day.spinner_comet.to_hex()
        ));
        out.push_str(&format!(
            "spinner_track = {:?}\n",
            day.spinner_track.to_hex()
        ));
        out.push_str(&format!("spinner_trail = {}\n", day.spinner_trail));
        out.push_str(&format!("comet_color = {:?}\n", day.comet_color.to_hex()));
        out.push_str(&format!("glow_color = {:?}\n", day.glow_color.to_hex()));
        out.push_str(&format!("sky_glow = {}\n", day.sky_glow));
        out.push_str(&format!("error_color = {:?}\n", day.error_color.to_hex()));
        out.push_str(&format!("logo_box = {:?}\n", day.logo_box.to_hex()));
        out
    }

    /// Render this theme as a documented `greeter.toml` — what `door-settings`
    /// writes. Mirrors the packaged default's layout so a saved file stays readable
    /// and re-editable by hand.
    pub fn to_config_string(&self) -> String {
        let mut out = String::new();
        out.push_str("# door greeter theme — written by door-settings. Edit here or in\n");
        out.push_str("# door-settings; keys are documented in the packaged default at\n");
        out.push_str(
            "# /usr/share/door/greeter.toml. Colors are \"#rrggbb\" or \"#rrggbbaa\".\n\n",
        );
        match &self.wallpaper {
            Some(w) => out.push_str(&format!("wallpaper = {:?}\n", w.display().to_string())),
            None => out.push_str("# wallpaper =   # (none — solid background)\n"),
        }
        out.push_str(&format!("background  = {:?}\n", self.background.to_hex()));
        out.push_str(&format!("card        = {:?}\n", self.card.to_hex()));
        out.push_str(&format!("field       = {:?}\n", self.field.to_hex()));
        out.push_str(&format!("accent      = {:?}\n", self.accent.to_hex()));
        out.push_str(&format!("foreground  = {:?}\n", self.foreground.to_hex()));
        out.push_str(&format!("muted       = {:?}\n", self.muted.to_hex()));
        match &self.font {
            Some(f) => out.push_str(&format!("font = {f:?}\n")),
            None => out.push_str("# font =   # (stock)\n"),
        }
        match &self.logo {
            Some(l) => out.push_str(&format!("logo = {:?}\n", l.display().to_string())),
            None => out.push_str("# logo =\n"),
        }
        out.push_str(&format!("corner_radius = {}\n", self.corner_radius));
        out.push_str(&format!("card_width    = {}\n", self.card_width));
        out.push_str(&format!("card_pos      = {:?}\n", self.card_pos.name()));
        out.push_str(&format!("show_clock    = {}\n", self.show_clock));
        out.push_str(&format!("animate       = {}\n", self.animate));
        out.push_str(&format!("sky_mode      = {:?}\n", self.sky_mode.name()));
        out.push_str(&format!("reduced_motion = {}\n", self.reduced_motion));
        out.push_str(&format!("gpu_level     = {:?}\n", self.gpu_level.name()));
        out.push_str(&format!("spinner_glow  = {}\n", self.spinner_glow));
        out.push_str(&format!("spinner_speed = {}\n", self.spinner_speed));
        out.push_str(&format!(
            "spinner_comet = {:?}\n",
            self.spinner_comet.to_hex()
        ));
        out.push_str(&format!(
            "spinner_track = {:?}\n",
            self.spinner_track.to_hex()
        ));
        out.push_str(&format!("spinner_trail = {}\n", self.spinner_trail));
        out.push_str(&format!(
            "comet_color   = {:?}\n",
            self.comet_color.to_hex()
        ));
        out.push('\n');
        out.push_str("# Sky\n");
        out.push_str(&format!("glow_color    = {:?}\n", self.glow_color.to_hex()));
        out.push_str(&format!("sky_glow      = {}\n", self.sky_glow));
        out.push_str(&format!("star_density  = {}\n", self.star_density));
        out.push_str(&format!("star_twinkle  = {}\n", self.star_twinkle));
        out.push_str(&format!("comet_enabled = {}\n", self.comet_enabled));
        out.push_str(&format!("comet_interval = {}\n", self.comet_interval));
        out.push_str(&format!("cloud_amount  = {}\n", self.cloud_amount));
        out.push_str(&format!("cloud_speed   = {}\n", self.cloud_speed));
        out.push_str(&format!("sun_x         = {}\n", self.sun_x));
        out.push_str(&format!("sun_y         = {}\n", self.sun_y));
        out.push_str(&format!("sun_size      = {}\n", self.sun_size));
        out.push_str(&format!("sun_intensity = {}\n", self.sun_intensity));
        out.push_str(&format!("sun_color     = {:?}\n", self.sun_color.to_hex()));
        out.push_str(&format!("glow_x        = {}\n", self.glow_x));
        out.push_str(&format!("glow_y        = {}\n", self.glow_y));
        out.push_str(&format!("day_haze      = {}\n", self.day_haze));
        out.push_str(&format!("star_layers   = {}\n", self.star_layers));
        out.push_str(&format!("star_size     = {}\n", self.star_size));
        out.push_str(&format!("nebula_speed  = {}\n", self.nebula_speed));
        out.push_str(&format!("comet_tilt    = {}\n", self.comet_tilt));
        out.push_str(&format!("comet_pause   = {}\n", self.comet_pause));
        out.push_str(&format!("comet_width   = {}\n", self.comet_width));
        out.push_str(&format!("cloud_lit     = {:?}\n", self.cloud_lit.to_hex()));
        out.push_str(&format!("cloud_shadow  = {:?}\n", self.cloud_shadow.to_hex()));
        out.push_str(&format!("cursor_parallax = {}\n", self.cursor_parallax));
        out.push_str(&format!("glow_pulse    = {}\n", self.glow_pulse));
        out.push_str(&format!("synthwave_grid_speed       = {}\n", self.synthwave_grid_speed));
        out.push_str(&format!("synthwave_grid_density     = {}\n", self.synthwave_grid_density));
        out.push_str(&format!("synthwave_grid_perspective = {}\n", self.synthwave_grid_perspective));
        out.push_str(&format!("synthwave_grid_glow        = {}\n", self.synthwave_grid_glow));
        out.push_str(&format!("synthwave_sun_size         = {}\n", self.synthwave_sun_size));
        out.push_str(&format!("synthwave_sun_stripes      = {}\n", self.synthwave_sun_stripes));
        out.push_str(&format!("synthwave_sun_bloom        = {}\n", self.synthwave_sun_bloom));
        out.push_str(&format!("synthwave_horizon          = {}\n", self.synthwave_horizon));
        out.push_str(&format!("synthwave_grid_color  = {:?}\n", self.synthwave_grid_color.to_hex()));
        out.push_str(&format!("synthwave_sky_top     = {:?}\n", self.synthwave_sky_top.to_hex()));
        out.push_str(&format!("synthwave_sky_bottom  = {:?}\n", self.synthwave_sky_bottom.to_hex()));
        out.push_str(&format!("storm_lightning_rate  = {}\n", self.storm_lightning_rate));
        out.push_str(&format!("storm_strike_chance   = {}\n", self.storm_strike_chance));
        out.push_str(&format!("storm_cloud_density   = {}\n", self.storm_cloud_density));
        out.push_str(&format!("storm_bolt_color      = {:?}\n", self.storm_bolt_color.to_hex()));
        out.push_str(&format!("storm_flash_color     = {:?}\n", self.storm_flash_color.to_hex()));
        out.push_str(&format!("rain_fall_speed   = {}\n", self.rain_fall_speed));
        out.push_str(&format!("rain_density      = {}\n", self.rain_density));
        out.push_str(&format!("rain_slant        = {}\n", self.rain_slant));
        out.push_str(&format!("rain_intensity    = {}\n", self.rain_intensity));
        out.push_str(&format!("rain_color        = {:?}\n", self.rain_color.to_hex()));
        out.push_str(&format!("snow_fall_speed   = {}\n", self.snow_fall_speed));
        out.push_str(&format!("snow_density      = {}\n", self.snow_density));
        out.push_str(&format!("snow_sway         = {}\n", self.snow_sway));
        out.push_str(&format!("snow_flake_size   = {}\n", self.snow_flake_size));
        out.push_str(&format!("snow_color        = {:?}\n", self.snow_color.to_hex()));
        out.push_str(&format!("fire_rise_speed   = {}\n", self.fire_rise_speed));
        out.push_str(&format!("fire_flame_height = {}\n", self.fire_flame_height));
        out.push_str(&format!("fire_flame_color  = {:?}\n", self.fire_flame_color.to_hex()));
        out.push_str(&format!("fire_tip_color    = {:?}\n", self.fire_tip_color.to_hex()));
        out.push_str(&format!("aurora_speed      = {}\n", self.aurora_speed));
        out.push_str(&format!("aurora_drop       = {}\n", self.aurora_drop));
        out.push_str(&format!("aurora_ray_freq   = {}\n", self.aurora_ray_freq));
        out.push_str(&format!("aurora_intensity  = {}\n", self.aurora_intensity));
        out.push_str(&format!("aurora_green      = {:?}\n", self.aurora_green.to_hex()));
        out.push_str(&format!("aurora_magenta    = {:?}\n", self.aurora_magenta.to_hex()));
        out.push_str(&format!("plasma_speed      = {}\n", self.plasma_speed));
        out.push_str(&format!("plasma_scale      = {}\n", self.plasma_scale));
        out.push_str(&format!("plasma_saturation = {}\n", self.plasma_saturation));
        out.push_str(&format!("plasma_tint       = {:?}\n", self.plasma_tint.to_hex()));
        out.push_str(&format!("water_speed       = {}\n", self.water_speed));
        out.push_str(&format!("water_scale       = {}\n", self.water_scale));
        out.push_str(&format!("water_ripple      = {}\n", self.water_ripple));
        out.push_str(&format!("water_caustic     = {}\n", self.water_caustic));
        out.push_str(&format!("water_caustic_color = {:?}\n", self.water_caustic_color.to_hex()));
        out.push_str(&format!("water_deep        = {:?}\n", self.water_deep.to_hex()));
        out.push_str(&format!("water_shallow     = {:?}\n", self.water_shallow.to_hex()));
        out.push_str(&format!("card_gradient = {}\n", self.card_gradient));
        out.push_str(&format!("grain         = {}\n", self.grain));
        out.push_str(&format!("vignette      = {}\n", self.vignette));
        out.push_str(&format!("card_blur     = {}\n", self.card_blur));
        out.push_str("# Spinner / card / behavior\n");
        out.push_str(&format!("spinner_size  = {}\n", self.spinner_size));
        out.push_str(&format!("spinner_pulse = {}\n", self.spinner_pulse));
        out.push_str(&format!("spinner_orbit = {}\n", self.spinner_orbit));
        out.push_str(&format!("spinner_style = {:?}\n", self.spinner_style.name()));
        out.push_str(&format!(
            "card_shadow_blur    = {}\n",
            self.card_shadow_blur
        ));
        out.push_str(&format!(
            "card_shadow_opacity = {}\n",
            self.card_shadow_opacity
        ));
        out.push_str(&format!("accent_breathing = {}\n", self.accent_breathing));
        out.push_str(&format!("field_radius  = {}\n", self.field_radius));
        out.push_str(&format!(
            "error_color   = {:?}\n",
            self.error_color.to_hex()
        ));
        out.push_str(&format!("logo_box      = {:?}\n", self.logo_box.to_hex()));
        out.push_str(&format!("logo_box_radius = {}\n", self.logo_box_radius));
        out.push_str(&format!("clock_24h     = {}\n", self.clock_24h));
        out.push_str(&format!("clock_seconds = {}\n", self.clock_seconds));
        out.push_str(&format!("clock_size    = {}\n", self.clock_size));
        out.push_str(&format!("clock_style   = {:?}\n", self.clock_style.name()));
        match &self.clock_format {
            Some(f) => out.push_str(&format!("clock_format  = {f:?}\n")),
            None => out.push_str("# clock_format =   # (built-in HH:MM)\n"),
        }
        out.push_str(&format!("font_scale    = {}\n", self.font_scale));
        out.push_str(&format!("font_weight   = {:?}\n", self.font_weight.name()));
        out.push_str(&format!("fade_ms       = {}\n", self.fade_ms));
        out.push_str("# Expert\n");
        out.push_str(&format!("glow_falloff  = {}\n", self.glow_falloff));
        out.push_str(&format!("nebula_amount = {}\n", self.nebula_amount));
        out.push_str(&format!("comet_tail_decay = {}\n", self.comet_tail_decay));
        out.push_str(&format!("spinner_ring  = {}\n", self.spinner_ring));
        out
    }
}

/// Parse `"HH:MM"` to minutes-since-midnight; `None` if malformed.
fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// The configured day window in minutes (default 07:00–19:00).
fn day_window(file: &ThemeFile) -> (u32, u32) {
    let start = file
        .day_start
        .as_deref()
        .and_then(parse_hhmm)
        .unwrap_or(7 * 60);
    let end = file
        .day_end
        .as_deref()
        .and_then(parse_hhmm)
        .unwrap_or(19 * 60);
    (start, end)
}

/// Is `now` within `[start, end)`? Handles a window that wraps past midnight.
fn in_window(now: u32, start: u32, end: u32) -> bool {
    if start <= end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_six_and_eight_digit_hex() {
        assert_eq!(
            Color::parse("#7aa2f7"),
            Some(Color::rgba(0x7a, 0xa2, 0xf7, 0xff))
        );
        assert_eq!(
            Color::parse("#24283bd0"),
            Some(Color::rgba(0x24, 0x28, 0x3b, 0xd0))
        );
        assert_eq!(Color::parse("#FFFFFF"), Some(Color::rgb(255, 255, 255)));
    }

    #[test]
    fn hex_round_trips() {
        for s in ["#7aa2f7", "#24283bd0", "#16161e"] {
            assert_eq!(Color::parse(s).unwrap().to_hex(), s);
        }
    }

    #[test]
    fn rejects_malformed_hex() {
        for bad in ["7aa2f7", "#abc", "#xyzxyz", "#1234567", ""] {
            assert_eq!(Color::parse(bad), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn empty_file_is_the_default() {
        let merged = Theme::default().merged(ThemeFile::default());
        assert_eq!(merged, Theme::default());
    }

    #[test]
    fn partial_file_overlays_only_named_fields() {
        let file: ThemeFile = toml::from_str(
            r##"
            accent = "#ff0000"
            card_width = 420.0
            show_clock = false
            wallpaper = "/usr/share/door/bg.png"
            "##,
        )
        .unwrap();
        let merged = Theme::default().merged(file);
        assert_eq!(merged.accent, Color::rgb(0xff, 0, 0));
        assert_eq!(merged.card_width, 420.0);
        assert!(!merged.show_clock);
        assert_eq!(
            merged.wallpaper,
            Some(PathBuf::from("/usr/share/door/bg.png"))
        );
        assert_eq!(merged.background, Theme::default().background);
        assert_eq!(merged.foreground, Theme::default().foreground);
    }

    #[test]
    fn a_bad_color_keeps_the_default_for_that_field() {
        let file: ThemeFile = toml::from_str(r#"accent = "not-a-color""#).unwrap();
        let merged = Theme::default().merged(file);
        assert_eq!(merged.accent, Theme::default().accent);
    }

    #[test]
    fn written_config_round_trips_through_parsing() {
        // What door-settings writes must parse back to the same theme.
        let t = Theme {
            accent: Color::rgb(0xbb, 0x9a, 0xf7),
            wallpaper: Some(PathBuf::from("/usr/share/door/wallpaper.png")),
            font: Some("MesloLGS Nerd Font".to_string()),
            show_clock: false,
            ..Default::default()
        };
        let rendered = t.to_config_string();
        let file: ThemeFile = toml::from_str(&rendered).expect("written config must parse");
        assert_eq!(Theme::default().merged(file), t);
    }

    #[test]
    fn shipped_default_config_parses_and_is_complete() {
        let raw = include_str!("../../dist/door/greeter.toml");
        let file: ThemeFile = toml::from_str(raw).expect("shipped greeter.toml must parse");
        let theme = Theme::default().merged(file);
        // Default is the clean animated sky on a solid bg (no static wallpaper).
        assert!(
            theme.wallpaper.is_none(),
            "default uses the animated sky, no image"
        );
        assert!(theme.animate);
        assert!(theme.show_clock);
        assert_eq!(theme.accent, Color::rgb(0x7a, 0xa2, 0xf7));
    }

    #[test]
    fn shipped_presets_parse_as_pairs() {
        // Every packaged preset must parse as a day+night pair (auto-covers new ones).
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../dist/door/presets");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).expect("presets dir must exist").flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).expect("read preset");
            Theme::parse_pair(&raw)
                .unwrap_or_else(|e| panic!("preset {:?} must parse: {e}", path.file_name().unwrap()));
            count += 1;
        }
        assert!(count >= 12, "expected the preset library, found only {count}");
    }

    #[test]
    fn day_window_parsing_and_membership() {
        assert_eq!(parse_hhmm("07:00"), Some(420));
        assert_eq!(parse_hhmm("19:30"), Some(1170));
        assert_eq!(parse_hhmm("nope"), None);
        assert_eq!(parse_hhmm("24:00"), None);
        // default 07:00–19:00
        assert!(in_window(12 * 60, 420, 1140)); // noon = day
        assert!(!in_window(6 * 60, 420, 1140)); // 06:00 = night
        assert!(!in_window(20 * 60, 420, 1140)); // 20:00 = night
                                                 // a window that wraps midnight
        assert!(in_window(23 * 60, 22 * 60, 5 * 60));
        assert!(in_window(2 * 60, 22 * 60, 5 * 60));
        assert!(!in_window(12 * 60, 22 * 60, 5 * 60));
    }

    #[test]
    fn render_pair_round_trips_both_palettes_and_window() {
        let night = Theme {
            accent: Color::rgb(0xbb, 0x9a, 0xf7),
            ..Default::default()
        };
        let day = Theme {
            accent: Color::rgb(0x12, 0x34, 0x56),
            background: Color::rgb(0xff, 0xff, 0xff),
            ..Theme::day()
        };
        let s = Theme::render_pair(&night, &day, "06:30", "18:45");
        let file: ThemeFile = toml::from_str(&s).expect("render_pair must parse");
        let n2 = Theme::default().merged(file.clone());
        let d2 = Theme::day()
            .merged_structural(&file)
            .merged_day(file.day.clone());
        assert_eq!(n2.accent, night.accent);
        assert_eq!(d2.accent, day.accent);
        assert_eq!(d2.background, day.background);
        assert_eq!(day_window(&file), (6 * 60 + 30, 18 * 60 + 45));
    }

    #[test]
    fn day_variant_is_light_and_flagged() {
        let day = Theme::day();
        assert!(day.is_day);
        assert!(!Theme::default().is_day);
        // Day background is light; night is dark.
        assert!(day.background.r > Theme::default().background.r);
    }

    #[test]
    fn unknown_keys_are_a_parse_error() {
        let result = toml::from_str::<ThemeFile>(r##"acent = "#ff0000""##);
        assert!(result.is_err());
    }
}
