//! door-settings — a standalone editor for the greeter theme (night + day).
//!
//! Loads both palettes ([`door_theme::Theme::load_pair`]), edits either via a
//! night/day toggle with a live in-window preview (real wallpaper + animated sky +
//! a themed mock card from the current draft), and saves both to
//! `/etc/door/greeter.toml` via `pkexec` (root-owned — the greeter is pre-login).
//! Sharing `door-theme` keeps one source of truth for the schema and the look.

use std::path::PathBuf;
use std::time::Instant;

use iced::widget::{
    button, canvas, column, combo_box, container, image, pick_list, row, scrollable, shader,
    slider, svg, text, text_input, toggler, tooltip, Column, Row, Space, Stack,
};
use iced::{
    mouse, Alignment, Background, Border, Color as IColor, ContentFit, Element, Length, Point,
    Rectangle, Renderer, Shadow, Subscription, Task, Vector,
};

use door_theme::skyshader::{FrostShader, SkyShader, SpinnerShader};
// Drop-in button whose style springs between hover/press states (eased transitions).
use iced_anim::widget::button as anim_button;

use door_theme::clock::AnalogClock;
use door_theme::{CardPos, ClockStyle, Color, FontWeight, SkyMode, SpinnerStyle, Theme};

fn main() -> iced::Result {
    iced::application(State::new, update, view)
        .title("door — greeter settings")
        .theme(app_theme)
        .style(app_style)
        .window_size((1180.0, 940.0))
        .subscription(subscription)
        .run()
}

/// Dark base theme so the Help tab's Markdown text (which uses the theme's text
/// color, not a per-widget style) renders light on the glass panel.
fn app_theme(_state: &State) -> iced::Theme {
    iced::Theme::TokyoNight
}

fn app_style(state: &State, _theme: &iced::Theme) -> iced::theme::Style {
    // Use the previewed variant's background so the day preview sits on a light bg
    // (not a hardcoded dark one) — the sky + card render over it accurately.
    let t = &state.preview;
    iced::theme::Style {
        background_color: t.background.iced(),
        text_color: t.foreground.iced(),
    }
}

/// A per-variant palette (the visual fields that differ between night and day).
#[derive(Default)]
struct Palette {
    wallpaper: String,
    background: String,
    card: String,
    field: String,
    accent: String,
    foreground: String,
    muted: String,
    logo: String,
    // Per-variant spinner fields.
    comet: String,
    track: String,
    trail: f32,
    glow: f32,
    // The background-sky comet color (per variant).
    comet_color: String,
    // Per-variant sky glow color + strength + login-error color.
    glow_color: String,
    sky_glow: f32,
    error_color: String,
    // Backdrop tile behind the logo / spinner (per variant; transparent = off).
    logo_box: String,
}

impl Palette {
    fn from_theme(t: &Theme) -> Self {
        let path = |p: &Option<PathBuf>| {
            p.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        };
        Palette {
            wallpaper: path(&t.wallpaper),
            background: t.background.to_hex(),
            card: t.card.to_hex(),
            field: t.field.to_hex(),
            accent: t.accent.to_hex(),
            foreground: t.foreground.to_hex(),
            muted: t.muted.to_hex(),
            logo: path(&t.logo),
            comet: t.spinner_comet.to_hex(),
            track: t.spinner_track.to_hex(),
            trail: t.spinner_trail,
            glow: t.spinner_glow,
            comet_color: t.comet_color.to_hex(),
            glow_color: t.glow_color.to_hex(),
            sky_glow: t.sky_glow,
            error_color: t.error_color.to_hex(),
            logo_box: t.logo_box.to_hex(),
        }
    }
}

/// Which field changed (palette fields route to the active variant; the rest are
/// shared across both).
#[derive(Debug, Clone, Copy)]
enum Param {
    Wallpaper,
    Background,
    Card,
    Field,
    Accent,
    Foreground,
    Muted,
    Logo,
    SpinnerComet,
    SpinnerTrack,
    SkyComet,
    GlowColor,
    ErrorColor,
    LogoBox,
    Font,
    CornerRadius,
    CardWidth,
    DayStart,
    DayEnd,
}

/// The control panel's tabs — splits the (long) option list so a tab fits the window
/// without scrolling. The header (variant toggle) and footer (actions) stay pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Colors,
    Sky,
    Spinner,
    Card,
    Behavior,
    Presets,
    Help,
}

#[derive(Debug, Clone)]
enum Message {
    Set(Param, String),
    CardAlpha(f32),
    SpinnerGlow(f32),
    SpinnerSpeed(f32),
    SpinnerTrail(f32),
    EditDay(bool),
    ToggleClock(bool),
    ToggleAnimate(bool),
    ToggleReducedMotion(bool),
    SkyModePicked(SkyMode),
    SelectTab(Tab),
    CardPosPicked(CardPos),
    ToggleHelp(bool),
    ToggleExpert(bool),
    // Sky
    SkyGlow(f32),
    StarDensity(f32),
    StarTwinkle(f32),
    CometEnabled(bool),
    CometInterval(f32),
    CloudAmount(f32),
    CloudSpeed(f32),
    SunX(f32),
    SunY(f32),
    SunSize(f32),
    SunIntensity(f32),
    SunColor(String),
    GlowX(f32),
    GlowY(f32),
    DayHaze(f32),
    StarLayers(f32),
    StarSize(f32),
    NebulaSpeed(f32),
    CometTilt(f32),
    CometPause(f32),
    CometWidth(f32),
    CloudLit(String),
    CloudShadow(String),
    CursorParallax(f32),
    GlowPulse(f32),
    SwGridSpeed(f32),
    SwGridDensity(f32),
    SwGridPerspective(f32),
    SwGridGlow(f32),
    SwSunSize(f32),
    SwSunStripes(f32),
    SwSunBloom(f32),
    SwHorizon(f32),
    SwGridColor(String),
    SwSkyTop(String),
    SwSkyBottom(String),
    CardGradient(f32),
    Grain(f32),
    Vignette(f32),
    SpinnerOrbit(f32),
    // Spinner
    SpinnerStylePicked(SpinnerStyle),
    SpinnerSize(f32),
    SpinnerPulse(f32),
    // Card
    CardShadowBlur(f32),
    CardShadowOpacity(f32),
    AccentBreathing(f32),
    FieldRadius(f32),
    LogoBoxRadius(f32),
    // Behavior
    Clock24h(bool),
    ClockSeconds(bool),
    ClockSize(f32),
    ClockStylePicked(ClockStyle),
    ClockFormat(String),
    FontScale(f32),
    FontWeightPicked(FontWeight),
    CardBlur(bool),
    FadeMs(f32),
    // Expert
    GlowFalloff(f32),
    NebulaAmount(f32),
    CometTailDecay(f32),
    SpinnerRing(f32),
    Tick,
    OpenInGreeter,
    Save,
    Reset,
    // Presets
    PresetPicked(Preset),
    PresetNameChanged(String),
    SavePreset,
    Randomize,
    // Visual color picker
    OpenPicker(Param),
    ClosePicker,
    // Import / export a preset file by path
    IoPathChanged(String),
    ImportPreset,
    ExportPreset,
    // A link in the Help tab's Markdown was clicked.
    HelpLink(String),
}

struct State {
    night: Palette,
    day: Palette,
    // Shared structural keys.
    font: String,
    corner_radius: String,
    card_width: String,
    card_pos: CardPos,
    day_start: String,
    day_end: String,
    spinner_speed: f32,
    show_clock: bool,
    animate: bool,
    sky_mode: SkyMode,
    reduced_motion: bool,
    // Shared sky / spinner / card / behavior controls (Tier 1+2).
    star_density: f32,
    star_twinkle: f32,
    comet_enabled: bool,
    comet_interval: f32,
    cloud_amount: f32,
    cloud_speed: f32,
    sun_x: f32,
    sun_y: f32,
    sun_size: f32,
    sun_intensity: f32,
    sun_color: String,
    glow_x: f32,
    glow_y: f32,
    day_haze: f32,
    star_layers: f32,
    star_size: f32,
    nebula_speed: f32,
    comet_tilt: f32,
    comet_pause: f32,
    comet_width: f32,
    cloud_lit: String,
    cloud_shadow: String,
    cursor_parallax: f32,
    glow_pulse: f32,
    // Synthwave scene controls (M8 pilot).
    synthwave_grid_speed: f32,
    synthwave_grid_density: f32,
    synthwave_grid_perspective: f32,
    synthwave_grid_glow: f32,
    synthwave_sun_size: f32,
    synthwave_sun_stripes: f32,
    synthwave_sun_bloom: f32,
    synthwave_horizon: f32,
    synthwave_grid_color: String,
    synthwave_sky_top: String,
    synthwave_sky_bottom: String,
    spinner_style: SpinnerStyle,
    card_gradient: f32,
    grain: f32,
    vignette: f32,
    card_blur: bool,
    spinner_orbit: f32,
    spinner_size: f32,
    spinner_pulse: f32,
    card_shadow_blur: f32,
    card_shadow_opacity: f32,
    accent_breathing: f32,
    field_radius: f32,
    logo_box_radius: f32,
    clock_24h: bool,
    clock_seconds: bool,
    clock_size: f32,
    clock_style: ClockStyle,
    clock_format: String,
    font_scale: f32,
    font_weight: FontWeight,
    fade_ms: f32,
    // Expert (advanced) shared controls (Tier 3).
    glow_falloff: f32,
    nebula_amount: f32,
    comet_tail_decay: f32,
    spinner_ring: f32,
    // Reveal the advanced controls (set by --expert).
    expert: bool,
    // Show one-line help under each control.
    help_on: bool,
    // Saved presets (both variants) + the name field for saving a new one.
    presets: Vec<Preset>,
    // Searchable picker state, mirroring `presets` (rebuilt when the list changes).
    preset_combo: combo_box::State<Preset>,
    selected_preset: Option<Preset>,
    preset_name: String,
    // Which color (if any) the visual HSV picker is open on.
    picking: Option<Param>,
    // Shared path field for importing / exporting a preset `.toml`.
    io_path: String,
    // Which variant is being edited / previewed.
    editing_day: bool,
    // Which control tab is showing.
    tab: Tab,
    status: String,
    anim: f32,
    started: Instant,
    // Live FPS of the preview (smoothed) + the last frame instant, for the badge.
    fps: f32,
    last_tick: Option<Instant>,
    /// Cached built theme for the preview/window — rebuilt on edits, not per frame
    /// (parsing every color twice each vsync frame was the day-flip stutter).
    preview: Theme,
    /// The Help tab's Markdown, parsed once (the `markdown` widget renders it).
    help_md: iced::widget::markdown::Content,
}

fn minutes_to_hhmm(m: u32) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// A saved theme preset (both variants + window), backed by a `.toml` file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Preset {
    name: String,
    path: PathBuf,
    /// The preset's night palette, parsed once at scan time — drives the live
    /// thumbnail mini-card. `None` if the file failed to parse.
    swatch: Option<PresetSwatch>,
}

/// The handful of colors a preset thumbnail needs (its night variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PresetSwatch {
    background: Color,
    card: Color,
    field: Color,
    accent: Color,
    foreground: Color,
}

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Expand a leading `~` to `$HOME` for the import/export path field.
fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(s)
}

/// `~/.config/door/presets` — the writable preset dir (created on save).
fn user_preset_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("door/presets"))
}

/// Discover presets: user dir (wins on name clash), then the packaged built-ins, plus
/// a `DOOR_PRESETS_DIR` override and the repo `dist/` for dev. Sorted, de-duped by name.
fn scan_presets() -> Vec<Preset> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("DOOR_PRESETS_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(d) = user_preset_dir() {
        dirs.push(d);
    }
    dirs.push(PathBuf::from("/usr/share/door/presets"));
    dirs.push(PathBuf::from("dist/door/presets"));

    let mut out: Vec<Preset> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("preset");
            let name = prettify(stem);
            if seen.insert(name.clone()) {
                // Parse the night palette now for the live thumbnail (cheap, ~small TOML).
                let swatch = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|c| Theme::parse_pair(&c).ok())
                    .map(|(night, _, _)| PresetSwatch {
                        background: night.background,
                        card: night.card,
                        field: night.field,
                        accent: night.accent,
                        foreground: night.foreground,
                    });
                out.push(Preset { name, path, swatch });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `tokyo-night` → `Tokyo Night` (filename stem → display name).
fn prettify(stem: &str) -> String {
    stem.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(c) => c.to_uppercase().chain(ch).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `My Theme!` → `my-theme` (display name → safe filename stem).
fn slugify(name: &str) -> String {
    let s: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

impl State {
    fn new() -> Self {
        let (night, day, (start, end)) = Theme::load_pair();
        let mut s = State {
            night: Palette::from_theme(&night),
            day: Palette::from_theme(&day),
            font: night.font.clone().unwrap_or_default(),
            corner_radius: night.corner_radius.to_string(),
            card_width: night.card_width.to_string(),
            card_pos: night.card_pos,
            day_start: minutes_to_hhmm(start),
            day_end: minutes_to_hhmm(end),
            spinner_speed: night.spinner_speed,
            show_clock: night.show_clock,
            animate: night.animate,
            sky_mode: night.sky_mode,
            reduced_motion: night.reduced_motion,
            star_density: night.star_density,
            star_twinkle: night.star_twinkle,
            comet_enabled: night.comet_enabled,
            comet_interval: night.comet_interval,
            cloud_amount: night.cloud_amount,
            cloud_speed: night.cloud_speed,
            sun_x: night.sun_x,
            sun_y: night.sun_y,
            sun_size: night.sun_size,
            sun_intensity: night.sun_intensity,
            sun_color: night.sun_color.to_hex(),
            glow_x: night.glow_x,
            glow_y: night.glow_y,
            day_haze: night.day_haze,
            star_layers: night.star_layers,
            star_size: night.star_size,
            nebula_speed: night.nebula_speed,
            comet_tilt: night.comet_tilt,
            comet_pause: night.comet_pause,
            comet_width: night.comet_width,
            cloud_lit: night.cloud_lit.to_hex(),
            cloud_shadow: night.cloud_shadow.to_hex(),
            cursor_parallax: night.cursor_parallax,
            glow_pulse: night.glow_pulse,
            synthwave_grid_speed: night.synthwave_grid_speed,
            synthwave_grid_density: night.synthwave_grid_density,
            synthwave_grid_perspective: night.synthwave_grid_perspective,
            synthwave_grid_glow: night.synthwave_grid_glow,
            synthwave_sun_size: night.synthwave_sun_size,
            synthwave_sun_stripes: night.synthwave_sun_stripes,
            synthwave_sun_bloom: night.synthwave_sun_bloom,
            synthwave_horizon: night.synthwave_horizon,
            synthwave_grid_color: night.synthwave_grid_color.to_hex(),
            synthwave_sky_top: night.synthwave_sky_top.to_hex(),
            synthwave_sky_bottom: night.synthwave_sky_bottom.to_hex(),
            spinner_style: night.spinner_style,
            card_gradient: night.card_gradient,
            grain: night.grain,
            vignette: night.vignette,
            card_blur: night.card_blur,
            spinner_orbit: night.spinner_orbit,
            spinner_size: night.spinner_size,
            spinner_pulse: night.spinner_pulse,
            card_shadow_blur: night.card_shadow_blur,
            card_shadow_opacity: night.card_shadow_opacity,
            accent_breathing: night.accent_breathing,
            field_radius: night.field_radius,
            logo_box_radius: night.logo_box_radius,
            clock_24h: night.clock_24h,
            clock_seconds: night.clock_seconds,
            clock_size: night.clock_size,
            clock_style: night.clock_style,
            clock_format: night.clock_format.clone().unwrap_or_default(),
            font_weight: night.font_weight,
            font_scale: night.font_scale,
            fade_ms: night.fade_ms,
            glow_falloff: night.glow_falloff,
            nebula_amount: night.nebula_amount,
            comet_tail_decay: night.comet_tail_decay,
            spinner_ring: night.spinner_ring,
            expert: std::env::args().any(|a| a == "--expert"),
            help_on: false,
            preset_combo: combo_box::State::new(scan_presets()),
            presets: scan_presets(),
            selected_preset: None,
            preset_name: String::new(),
            picking: None,
            io_path: String::new(),
            // Dev: start on the day variant when DOOR_SETTINGS_DAY is set.
            editing_day: std::env::var_os("DOOR_SETTINGS_DAY").is_some(),
            tab: Tab::Colors,
            status: "Loaded night + day themes.".to_string(),
            anim: 0.0,
            started: Instant::now(),
            fps: 0.0,
            last_tick: None,
            preview: Theme::default(),
            help_md: iced::widget::markdown::Content::parse(HELP_MD),
        };
        s.rebuild_preview();
        s
    }

    /// Replace every variant + shared field from a loaded pair (a preset or reload).
    fn apply_pair(&mut self, night: &Theme, day: &Theme, window: (u32, u32)) {
        self.night = Palette::from_theme(night);
        self.day = Palette::from_theme(day);
        self.font = night.font.clone().unwrap_or_default();
        self.corner_radius = night.corner_radius.to_string();
        self.card_width = night.card_width.to_string();
        self.card_pos = night.card_pos;
        self.day_start = minutes_to_hhmm(window.0);
        self.day_end = minutes_to_hhmm(window.1);
        self.spinner_speed = night.spinner_speed;
        self.show_clock = night.show_clock;
        self.animate = night.animate;
        self.sky_mode = night.sky_mode;
        self.reduced_motion = night.reduced_motion;
        self.star_density = night.star_density;
        self.star_twinkle = night.star_twinkle;
        self.comet_enabled = night.comet_enabled;
        self.comet_interval = night.comet_interval;
        self.cloud_amount = night.cloud_amount;
        self.cloud_speed = night.cloud_speed;
        self.sun_x = night.sun_x;
        self.sun_y = night.sun_y;
        self.sun_size = night.sun_size;
        self.sun_intensity = night.sun_intensity;
        self.sun_color = night.sun_color.to_hex();
        self.glow_x = night.glow_x;
        self.glow_y = night.glow_y;
        self.day_haze = night.day_haze;
        self.star_layers = night.star_layers;
        self.star_size = night.star_size;
        self.nebula_speed = night.nebula_speed;
        self.comet_tilt = night.comet_tilt;
        self.comet_pause = night.comet_pause;
        self.comet_width = night.comet_width;
        self.cloud_lit = night.cloud_lit.to_hex();
        self.cloud_shadow = night.cloud_shadow.to_hex();
        self.cursor_parallax = night.cursor_parallax;
        self.glow_pulse = night.glow_pulse;
        self.synthwave_grid_speed = night.synthwave_grid_speed;
        self.synthwave_grid_density = night.synthwave_grid_density;
        self.synthwave_grid_perspective = night.synthwave_grid_perspective;
        self.synthwave_grid_glow = night.synthwave_grid_glow;
        self.synthwave_sun_size = night.synthwave_sun_size;
        self.synthwave_sun_stripes = night.synthwave_sun_stripes;
        self.synthwave_sun_bloom = night.synthwave_sun_bloom;
        self.synthwave_horizon = night.synthwave_horizon;
        self.synthwave_grid_color = night.synthwave_grid_color.to_hex();
        self.synthwave_sky_top = night.synthwave_sky_top.to_hex();
        self.synthwave_sky_bottom = night.synthwave_sky_bottom.to_hex();
        self.spinner_style = night.spinner_style;
        self.card_gradient = night.card_gradient;
        self.grain = night.grain;
        self.vignette = night.vignette;
        self.card_blur = night.card_blur;
        self.spinner_orbit = night.spinner_orbit;
        self.spinner_size = night.spinner_size;
        self.spinner_pulse = night.spinner_pulse;
        self.card_shadow_blur = night.card_shadow_blur;
        self.card_shadow_opacity = night.card_shadow_opacity;
        self.accent_breathing = night.accent_breathing;
        self.field_radius = night.field_radius;
        self.logo_box_radius = night.logo_box_radius;
        self.clock_24h = night.clock_24h;
        self.clock_seconds = night.clock_seconds;
        self.clock_size = night.clock_size;
        self.clock_style = night.clock_style;
        self.clock_format = night.clock_format.clone().unwrap_or_default();
        self.font_weight = night.font_weight;
        self.font_scale = night.font_scale;
        self.fade_ms = night.fade_ms;
        self.glow_falloff = night.glow_falloff;
        self.nebula_amount = night.nebula_amount;
        self.comet_tail_decay = night.comet_tail_decay;
        self.spinner_ring = night.spinner_ring;
    }

    /// Recompute the cached preview theme (call after any edit that affects it).
    fn rebuild_preview(&mut self) {
        self.preview = self.build(self.editing_day).unwrap_or_else(|_| {
            if self.editing_day {
                Theme::day()
            } else {
                Theme::default()
            }
        });
        // The preview reflects reduced-motion stillness; the saved config keeps the
        // real values (build() returns them raw for render_pair).
        self.preview.apply_reduced_motion();
    }

    fn active(&self) -> &Palette {
        if self.editing_day {
            &self.day
        } else {
            &self.night
        }
    }

    fn set(&mut self, param: Param, value: String) {
        let pal = if self.editing_day {
            &mut self.day
        } else {
            &mut self.night
        };
        match param {
            Param::Wallpaper => pal.wallpaper = value,
            Param::Background => pal.background = value,
            Param::Card => pal.card = value,
            Param::Field => pal.field = value,
            Param::Accent => pal.accent = value,
            Param::Foreground => pal.foreground = value,
            Param::Muted => pal.muted = value,
            Param::Logo => pal.logo = value,
            Param::SpinnerComet => pal.comet = value,
            Param::SpinnerTrack => pal.track = value,
            Param::SkyComet => pal.comet_color = value,
            Param::GlowColor => pal.glow_color = value,
            Param::ErrorColor => pal.error_color = value,
            Param::LogoBox => pal.logo_box = value,
            Param::Font => self.font = value,
            Param::CornerRadius => self.corner_radius = value,
            Param::CardWidth => self.card_width = value,
            Param::DayStart => self.day_start = value,
            Param::DayEnd => self.day_end = value,
        }
    }

    /// The current hex string for a color [`Param`] in the variant being edited —
    /// the read counterpart to [`set`](Self::set), for the visual picker. Non-color
    /// params return empty (the picker only opens on colors).
    fn color_hex(&self, param: Param) -> &str {
        let pal = if self.editing_day { &self.day } else { &self.night };
        match param {
            Param::Background => &pal.background,
            Param::Card => &pal.card,
            Param::Field => &pal.field,
            Param::Accent => &pal.accent,
            Param::Foreground => &pal.foreground,
            Param::Muted => &pal.muted,
            Param::SpinnerComet => &pal.comet,
            Param::SpinnerTrack => &pal.track,
            Param::SkyComet => &pal.comet_color,
            Param::GlowColor => &pal.glow_color,
            Param::ErrorColor => &pal.error_color,
            Param::LogoBox => &pal.logo_box,
            _ => "",
        }
    }

    /// Build one variant's [`Theme`] from its palette + the shared structural keys.
    fn build(&self, day: bool) -> Result<Theme, String> {
        let pal = if day { &self.day } else { &self.night };
        let color = |label: &str, v: &str| {
            Color::parse(v.trim()).ok_or_else(|| format!("{label}: '{v}' is not #rrggbb[aa]"))
        };
        let opt_path = |v: &str| {
            let t = v.trim();
            (!t.is_empty()).then(|| PathBuf::from(t))
        };
        let num = |label: &str, v: &str| {
            v.trim()
                .parse::<f32>()
                .map_err(|_| format!("{label}: '{v}' is not a number"))
        };
        Ok(Theme {
            wallpaper: opt_path(&pal.wallpaper),
            background: color("Background", &pal.background)?,
            card: color("Card", &pal.card)?,
            field: color("Field", &pal.field)?,
            accent: color("Accent", &pal.accent)?,
            foreground: color("Foreground", &pal.foreground)?,
            muted: color("Muted", &pal.muted)?,
            logo: opt_path(&pal.logo),
            font: {
                let t = self.font.trim();
                (!t.is_empty()).then(|| t.to_string())
            },
            corner_radius: num("Corner radius", &self.corner_radius)?,
            card_width: num("Card width", &self.card_width)?,
            card_pos: self.card_pos,
            show_clock: self.show_clock,
            animate: self.animate,
            is_day: day,
            sky_mode: self.sky_mode,
            reduced_motion: self.reduced_motion,
            spinner_glow: pal.glow,
            spinner_speed: self.spinner_speed,
            spinner_comet: color("Comet", &pal.comet)?,
            spinner_track: color("Track", &pal.track)?,
            spinner_trail: pal.trail,
            comet_color: color("Sky comet", &pal.comet_color)?,
            glow_color: color("Glow color", &pal.glow_color)?,
            sky_glow: pal.sky_glow,
            error_color: color("Error", &pal.error_color)?,
            logo_box: color("Logo box", &pal.logo_box)?,
            logo_box_radius: self.logo_box_radius,
            star_density: self.star_density,
            star_twinkle: self.star_twinkle,
            comet_enabled: self.comet_enabled,
            comet_interval: self.comet_interval,
            cloud_amount: self.cloud_amount,
            cloud_speed: self.cloud_speed,
            sun_x: self.sun_x,
            sun_y: self.sun_y,
            sun_size: self.sun_size,
            sun_intensity: self.sun_intensity,
            sun_color: color("Sun color", &self.sun_color)?,
            glow_x: self.glow_x,
            glow_y: self.glow_y,
            day_haze: self.day_haze,
            star_layers: self.star_layers,
            star_size: self.star_size,
            nebula_speed: self.nebula_speed,
            comet_tilt: self.comet_tilt,
            comet_pause: self.comet_pause,
            comet_width: self.comet_width,
            cloud_lit: color("Cloud lit", &self.cloud_lit)?,
            cloud_shadow: color("Cloud shadow", &self.cloud_shadow)?,
            cursor_parallax: self.cursor_parallax,
            glow_pulse: self.glow_pulse,
            synthwave_grid_speed: self.synthwave_grid_speed,
            synthwave_grid_density: self.synthwave_grid_density,
            synthwave_grid_perspective: self.synthwave_grid_perspective,
            synthwave_grid_glow: self.synthwave_grid_glow,
            synthwave_sun_size: self.synthwave_sun_size,
            synthwave_sun_stripes: self.synthwave_sun_stripes,
            synthwave_sun_bloom: self.synthwave_sun_bloom,
            synthwave_horizon: self.synthwave_horizon,
            synthwave_grid_color: color("Synthwave grid", &self.synthwave_grid_color)?,
            synthwave_sky_top: color("Synthwave sky top", &self.synthwave_sky_top)?,
            synthwave_sky_bottom: color("Synthwave sky bottom", &self.synthwave_sky_bottom)?,
            spinner_style: self.spinner_style,
            card_gradient: self.card_gradient,
            grain: self.grain,
            vignette: self.vignette,
            card_blur: self.card_blur,
            spinner_orbit: self.spinner_orbit,
            spinner_size: self.spinner_size,
            spinner_pulse: self.spinner_pulse,
            card_shadow_blur: self.card_shadow_blur,
            card_shadow_opacity: self.card_shadow_opacity,
            accent_breathing: self.accent_breathing,
            field_radius: self.field_radius,
            clock_24h: self.clock_24h,
            clock_seconds: self.clock_seconds,
            clock_size: self.clock_size,
            clock_style: self.clock_style,
            clock_format: {
                let t = self.clock_format.trim();
                (!t.is_empty()).then(|| t.to_string())
            },
            font_weight: self.font_weight,
            font_scale: self.font_scale,
            fade_ms: self.fade_ms,
            glow_falloff: self.glow_falloff,
            nebula_amount: self.nebula_amount,
            comet_tail_decay: self.comet_tail_decay,
            spinner_ring: self.spinner_ring,
        })
    }
}

/// Render both palettes to a full `greeter.toml`, returning the temp path.
fn write_draft(state: &State) -> Result<PathBuf, String> {
    let night = state.build(false)?;
    let day = state.build(true)?;
    let toml = Theme::render_pair(&night, &day, state.day_start.trim(), state.day_end.trim());
    let path = std::env::temp_dir().join("door-settings-draft.toml");
    std::fs::write(&path, toml).map_err(|e| format!("writing draft: {e}"))?;
    Ok(path)
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    // Every message except the per-frame animation tick can change the theme; only
    // rebuild the cached preview for those (not 60×/s) so the day flip stays smooth.
    let touches_theme = !matches!(
        message,
        Message::Tick
            | Message::SelectTab(_)
            | Message::ToggleHelp(_)
            | Message::ToggleExpert(_)
            | Message::PresetNameChanged(_)
            | Message::SavePreset
            | Message::OpenPicker(_)
            | Message::ClosePicker
            | Message::IoPathChanged(_)
            | Message::ExportPreset
            | Message::HelpLink(_)
    );
    match message {
        Message::Set(param, value) => state.set(param, value),
        Message::CardAlpha(v) => {
            let pal = if state.editing_day {
                &mut state.day
            } else {
                &mut state.night
            };
            if let Some(mut col) = Color::parse(pal.card.trim()) {
                col.a = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                pal.card = col.to_hex();
            }
        }
        Message::SpinnerGlow(v) => {
            let pal = if state.editing_day {
                &mut state.day
            } else {
                &mut state.night
            };
            pal.glow = v.clamp(0.0, 1.0);
        }
        Message::SpinnerSpeed(v) => state.spinner_speed = v.clamp(0.0, 8.0),
        Message::SpinnerTrail(v) => {
            let pal = if state.editing_day {
                &mut state.day
            } else {
                &mut state.night
            };
            pal.trail = v.clamp(0.15, 1.0);
        }
        Message::EditDay(on) => state.editing_day = on,
        Message::SelectTab(t) => state.tab = t,
        Message::CardPosPicked(p) => state.card_pos = p,
        Message::ToggleHelp(on) => state.help_on = on,
        Message::ToggleExpert(on) => state.expert = on,
        Message::ToggleClock(on) => state.show_clock = on,
        Message::ToggleAnimate(on) => state.animate = on,
        Message::ToggleReducedMotion(on) => state.reduced_motion = on,
        Message::SkyModePicked(m) => state.sky_mode = m,
        // Per-variant sky glow.
        Message::SkyGlow(v) => {
            let pal = if state.editing_day {
                &mut state.day
            } else {
                &mut state.night
            };
            pal.sky_glow = v.clamp(0.0, 2.0);
        }
        // Shared sky/spinner/card/behavior controls.
        Message::StarDensity(v) => state.star_density = v.clamp(0.0, 1.0),
        Message::StarTwinkle(v) => state.star_twinkle = v.clamp(0.0, 4.0),
        Message::CometEnabled(on) => state.comet_enabled = on,
        Message::CometInterval(v) => state.comet_interval = v.clamp(3.5, 30.0),
        Message::CloudAmount(v) => state.cloud_amount = v.clamp(0.0, 2.0),
        Message::CloudSpeed(v) => state.cloud_speed = v.clamp(0.0, 4.0),
        Message::SunX(v) => state.sun_x = v.clamp(0.0, 1.0),
        Message::SunY(v) => state.sun_y = v.clamp(0.0, 1.0),
        Message::SunSize(v) => state.sun_size = v.clamp(0.05, 0.6),
        Message::SunIntensity(v) => state.sun_intensity = v.clamp(0.0, 1.0),
        Message::SunColor(s) => state.sun_color = s,
        Message::GlowX(v) => state.glow_x = v.clamp(0.0, 1.0),
        Message::GlowY(v) => state.glow_y = v.clamp(0.0, 1.0),
        Message::DayHaze(v) => state.day_haze = v.clamp(0.0, 1.0),
        Message::StarLayers(v) => state.star_layers = v.round().clamp(1.0, 5.0),
        Message::StarSize(v) => state.star_size = v.clamp(0.3, 3.0),
        Message::NebulaSpeed(v) => state.nebula_speed = v.clamp(0.0, 4.0),
        Message::CometTilt(v) => state.comet_tilt = v.clamp(-1.57, 1.57),
        Message::CometPause(v) => state.comet_pause = v.clamp(0.0, 6.0),
        Message::CometWidth(v) => state.comet_width = v.clamp(0.3, 3.0),
        Message::CloudLit(s) => state.cloud_lit = s,
        Message::CloudShadow(s) => state.cloud_shadow = s,
        Message::CursorParallax(v) => state.cursor_parallax = v.clamp(0.0, 3.0),
        Message::GlowPulse(v) => state.glow_pulse = v.clamp(0.0, 2.0),
        Message::SwGridSpeed(v) => state.synthwave_grid_speed = v.clamp(0.0, 6.0),
        Message::SwGridDensity(v) => state.synthwave_grid_density = v.clamp(0.1, 2.0),
        Message::SwGridPerspective(v) => state.synthwave_grid_perspective = v.clamp(0.1, 2.0),
        Message::SwGridGlow(v) => state.synthwave_grid_glow = v.clamp(0.0, 2.0),
        Message::SwSunSize(v) => state.synthwave_sun_size = v.clamp(0.05, 0.45),
        Message::SwSunStripes(v) => state.synthwave_sun_stripes = v.clamp(0.0, 300.0),
        Message::SwSunBloom(v) => state.synthwave_sun_bloom = v.clamp(0.0, 1.0),
        Message::SwHorizon(v) => state.synthwave_horizon = v.clamp(0.2, 0.9),
        Message::SwGridColor(s) => state.synthwave_grid_color = s,
        Message::SwSkyTop(s) => state.synthwave_sky_top = s,
        Message::SwSkyBottom(s) => state.synthwave_sky_bottom = s,
        Message::CardGradient(v) => state.card_gradient = v.clamp(0.0, 1.0),
        Message::Grain(v) => state.grain = v.clamp(0.0, 0.3),
        Message::Vignette(v) => state.vignette = v.clamp(0.0, 1.0),
        Message::SpinnerOrbit(v) => state.spinner_orbit = v.clamp(0.1, 0.45),
        Message::SpinnerStylePicked(s) => state.spinner_style = s,
        Message::SpinnerSize(v) => state.spinner_size = v.clamp(24.0, 120.0),
        Message::SpinnerPulse(v) => state.spinner_pulse = v.clamp(0.0, 3.0),
        Message::CardShadowBlur(v) => state.card_shadow_blur = v.clamp(0.0, 80.0),
        Message::CardShadowOpacity(v) => state.card_shadow_opacity = v.clamp(0.0, 1.0),
        Message::AccentBreathing(v) => state.accent_breathing = v.clamp(0.0, 4.0),
        Message::FieldRadius(v) => state.field_radius = v.clamp(0.0, 30.0),
        Message::LogoBoxRadius(v) => state.logo_box_radius = v.clamp(0.0, 40.0),
        Message::Clock24h(on) => state.clock_24h = on,
        Message::ClockSeconds(on) => state.clock_seconds = on,
        Message::ClockSize(v) => state.clock_size = v.clamp(24.0, 120.0),
        Message::ClockStylePicked(s) => state.clock_style = s,
        Message::ClockFormat(s) => state.clock_format = s,
        Message::FontScale(v) => state.font_scale = v.clamp(0.7, 1.8),
        Message::FontWeightPicked(w) => state.font_weight = w,
        Message::CardBlur(on) => state.card_blur = on,
        Message::FadeMs(v) => state.fade_ms = v.clamp(0.0, 2000.0),
        // Expert.
        Message::GlowFalloff(v) => state.glow_falloff = v.clamp(0.5, 10.0),
        Message::NebulaAmount(v) => state.nebula_amount = v.clamp(0.0, 0.5),
        Message::CometTailDecay(v) => state.comet_tail_decay = v.clamp(2.0, 30.0),
        Message::SpinnerRing(v) => state.spinner_ring = v.clamp(0.0, 0.5),
        Message::Tick => {
            state.anim = state.started.elapsed().as_secs_f32() % 10_000.0;
            let now = Instant::now();
            if let Some(last) = state.last_tick {
                let dt = now.duration_since(last).as_secs_f32();
                if dt > 0.0 {
                    let inst = 1.0 / dt;
                    // EMA so the badge reads steadily, not jittery per-frame.
                    state.fps = if state.fps <= 0.0 {
                        inst
                    } else {
                        state.fps * 0.9 + inst * 0.1
                    };
                }
            }
            state.last_tick = Some(now);
        }
        Message::Reset => {
            let keep = state.editing_day;
            *state = State::new();
            state.editing_day = keep;
            state.status = "Reloaded the saved themes.".to_string();
        }
        Message::PresetNameChanged(s) => state.preset_name = s,
        Message::OpenPicker(param) => state.picking = Some(param),
        Message::ClosePicker => state.picking = None,
        Message::HelpLink(url) => {
            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
        }
        Message::IoPathChanged(s) => state.io_path = s,
        Message::ImportPreset => {
            let path = expand_tilde(state.io_path.trim());
            if state.io_path.trim().is_empty() {
                state.status = "Enter a path to import from.".into();
            } else {
                match std::fs::read_to_string(&path)
                    .map_err(|e| format!("reading {}: {e}", path.display()))
                    .and_then(|c| Theme::parse_pair(&c).map_err(|e| format!("parsing: {e}")))
                {
                    Ok((night, day, window)) => {
                        state.apply_pair(&night, &day, window);
                        state.selected_preset = None;
                        state.status = format!("Imported {}.", path.display());
                    }
                    Err(e) => state.status = format!("Import failed: {e}"),
                }
            }
        }
        Message::ExportPreset => {
            let path = expand_tilde(state.io_path.trim());
            if state.io_path.trim().is_empty() {
                state.status = "Enter a destination path to export to.".into();
            } else {
                match (state.build(false), state.build(true)) {
                    (Ok(night), Ok(day)) => {
                        let body =
                            Theme::render_pair(&night, &day, &state.day_start, &state.day_end);
                        match std::fs::write(&path, body) {
                            Ok(_) => state.status = format!("Exported to {}.", path.display()),
                            Err(e) => state.status = format!("Export failed: {e}"),
                        }
                    }
                    (Err(e), _) | (_, Err(e)) => state.status = format!("Fix before exporting: {e}"),
                }
            }
        }
        Message::Randomize => {
            if !state.presets.is_empty() {
                let n = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as usize)
                    .unwrap_or(0);
                let p = state.presets[n % state.presets.len()].clone();
                if let Ok(contents) = std::fs::read_to_string(&p.path) {
                    if let Ok((night, day, window)) = Theme::parse_pair(&contents) {
                        state.apply_pair(&night, &day, window);
                        state.preset_name = p.name.clone();
                        state.selected_preset = Some(p.clone());
                        state.status = format!("🎲 {}", p.name);
                    }
                }
            }
        }
        Message::PresetPicked(p) => match std::fs::read_to_string(&p.path) {
            Ok(contents) => match Theme::parse_pair(&contents) {
                Ok((night, day, window)) => {
                    state.apply_pair(&night, &day, window);
                    state.preset_name = p.name.clone();
                    state.selected_preset = Some(p.clone());
                    state.status = format!("Loaded preset '{}'.", p.name);
                }
                Err(e) => state.status = format!("Preset '{}' is malformed: {e}", p.name),
            },
            Err(e) => state.status = format!("Could not read preset: {e}"),
        },
        Message::SavePreset => {
            let slug = slugify(&state.preset_name);
            if slug.is_empty() {
                state.status = "Name the preset before saving.".into();
            } else {
                match (state.build(false), state.build(true)) {
                    (Ok(night), Ok(day)) => {
                        let body =
                            Theme::render_pair(&night, &day, &state.day_start, &state.day_end);
                        match user_preset_dir() {
                            Some(dir) => {
                                let path = dir.join(format!("{slug}.toml"));
                                match std::fs::create_dir_all(&dir)
                                    .and_then(|_| std::fs::write(&path, body))
                                {
                                    Ok(_) => {
                                        state.presets = scan_presets();
                                        state.preset_combo =
                                            combo_box::State::new(state.presets.clone());
                                        state.selected_preset =
                                            state.presets.iter().find(|p| p.path == path).cloned();
                                        state.status =
                                            format!("Saved preset '{}'.", prettify(&slug));
                                    }
                                    Err(e) => state.status = format!("Could not save preset: {e}"),
                                }
                            }
                            None => state.status = "No HOME to save the preset into.".into(),
                        }
                    }
                    (Err(e), _) | (_, Err(e)) => state.status = format!("Fix before saving: {e}"),
                }
            }
        }
        Message::OpenInGreeter => match write_draft(state) {
            Ok(path) => {
                let bin =
                    std::env::var("DOOR_GREETER_BIN").unwrap_or_else(|_| "door-greeter".into());
                match std::process::Command::new(&bin)
                    .env("DOORD_GREETER_DEV", "1")
                    .env("DOORD_GREETER_CONFIG", &path)
                    .spawn()
                {
                    Ok(_) => state.status = "Opened a full greeter window.".into(),
                    Err(e) => state.status = format!("Could not launch '{bin}': {e}"),
                }
            }
            Err(e) => state.status = e,
        },
        Message::Save => match write_draft(state) {
            Ok(path) => match std::process::Command::new("pkexec")
                .args(["install", "-Dm644"])
                .arg(&path)
                .arg(door_theme::ETC_CONFIG)
                .status()
            {
                Ok(s) if s.success() => {
                    state.status = format!("Saved night + day to {} ✓", door_theme::ETC_CONFIG)
                }
                Ok(_) => state.status = "Save cancelled or failed at the pkexec prompt.".into(),
                Err(e) => state.status = format!("Could not run pkexec: {e}"),
            },
            Err(e) => state.status = e,
        },
    }
    if touches_theme {
        state.rebuild_preview();
    }
    Task::none()
}

fn subscription(state: &State) -> Subscription<Message> {
    if state.animate {
        iced::window::frames().map(|_| Message::Tick)
    } else {
        Subscription::none()
    }
}

// ---- view -----------------------------------------------------------------

fn view(state: &State) -> Element<'_, Message> {
    let theme = &state.preview;

    let panel = container(controls(state))
        .width(Length::Fixed(560.0))
        .height(Length::Fill)
        .padding(16)
        .style(glass_panel);
    let left = container(panel).padding(16);

    let preview_inner: Element<Message> = if theme.card_blur {
        Stack::new()
            .push(preview_card(theme, state.anim))
            .push_under(
                shader(FrostShader::from_theme(theme, state.anim, 1.0))
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .into()
    } else {
        preview_card(theme, state.anim)
    };
    let pv = container(preview_inner).padding(24);
    let preview = match theme.card_pos {
        CardPos::Center => pv.center_x(Length::Fill).center_y(Length::Fill),
        CardPos::Left => pv.align_left(Length::Fill).center_y(Length::Fill),
        CardPos::Right => pv.align_right(Length::Fill).center_y(Length::Fill),
        CardPos::Top => pv.center_x(Length::Fill).align_top(Length::Fill),
        CardPos::Bottom => pv.center_x(Length::Fill).align_bottom(Length::Fill),
    };

    let content = row![left, preview].height(Length::Fill);

    let sky_layer: Element<Message> = if theme.animate {
        shader(SkyShader::from_theme(theme, state.anim, 1.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        // A Fill-size base, not a zero-size Space: an empty Space here collapses the
        // stack's layout and drops the content layer (whole window renders blank).
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    // FPS badge — a small live frame-rate readout in the top-right (perf while tuning).
    let fps_label = if theme.animate {
        format!("{:.0} fps", state.fps.max(0.0))
    } else {
        "paused".to_string()
    };
    let fps_badge = container(
        container(text(fps_label).size(11).color(c(0xc8, 0xcc, 0xd4)))
            .padding([3.0, 8.0])
            .style(|_t: &iced::Theme| container::Style {
                background: Some(Background::Color(IColor::from_rgba8(0x00, 0x00, 0x00, 0.45))),
                border: Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.18),
                },
                ..Default::default()
            }),
    )
    .align_right(Length::Fill)
    .align_top(Length::Fill)
    .padding(10);

    let base: Element<Message> = match &theme.wallpaper {
        Some(path) => {
            let bg = image(image::Handle::from_path(path))
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Cover);
            iced::widget::stack![bg, sky_layer, content, fps_badge].into()
        }
        None => iced::widget::stack![sky_layer, content, fps_badge].into(),
    };
    // The visual HSV picker floats over everything while a swatch is being edited.
    match state.picking {
        Some(param) => iced::widget::stack![base, picker_overlay(state, param)].into(),
        None => base,
    }
}

fn c(r: u8, g: u8, b: u8) -> IColor {
    IColor::from_rgb8(r, g, b)
}
const LABEL: (u8, u8, u8) = (0x9a, 0xa3, 0xc8);
const ACCENT: (u8, u8, u8) = (0x7a, 0xa2, 0xf7);
const FG: (u8, u8, u8) = (0xc0, 0xca, 0xf5);
const MUTED: (u8, u8, u8) = (0x56, 0x5f, 0x89);

fn controls(state: &State) -> Element<'_, Message> {
    let pal = state.active();
    let h = state.help_on;
    let card_a = Color::parse(pal.card.trim())
        .map(|col| col.a as f32 / 255.0)
        .unwrap_or(1.0);
    let editing_day = state.editing_day;

    // Header: a single compact row — title, the Night/Day variant toggle, and the
    // Help / Advanced toggles (Advanced reveals expert controls; also via `--expert`).
    let header = row![
        text("door").size(26).color(c(FG.0, FG.1, FG.2)),
        text("greeter").size(26).color(c(ACCENT.0, ACCENT.1, ACCENT.2)),
        Space::new().width(Length::Fill),
        tip(
            toggler(state.help_on)
                .label("Help")
                .on_toggle(Message::ToggleHelp)
                .size(16)
                .text_size(12),
            "Show a one-line description under each control"
        ),
        tip(
            toggler(state.expert)
                .label("Advanced")
                .on_toggle(Message::ToggleExpert)
                .size(16)
                .text_size(12),
            "Reveal the expert (Tier-3) controls"
        ),
        tip(
            toggler(editing_day)
                .label(if editing_day { "Day" } else { "Night" })
                .on_toggle(Message::EditDay)
                .size(16)
                .text_size(12),
            "Edit the night or day palette (the greeter picks by clock)"
        ),
    ]
    .spacing(14)
    .align_y(Alignment::Center);

    let tabbar = row![
        tab_button("Colors", Tab::Colors, state.tab),
        tab_button("Sky", Tab::Sky, state.tab),
        tab_button("Spinner", Tab::Spinner, state.tab),
        tab_button("Card", Tab::Card, state.tab),
        tab_button("Behavior", Tab::Behavior, state.tab),
        tab_button("Presets", Tab::Presets, state.tab),
        tab_button("Help", Tab::Help, state.tab),
    ]
    .spacing(5);

    let body: Element<Message> = match state.tab {
        Tab::Colors => colors_tab(state, pal, card_a, h),
        Tab::Sky => sky_tab(state, pal, h),
        Tab::Spinner => spinner_tab(state, pal, h),
        Tab::Card => card_tab(state, h),
        Tab::Behavior => behavior_tab(state, h),
        Tab::Presets => presets_tab(state),
        Tab::Help => help_tab(state),
    };

    let actions = row![
        tip(
            primary_button("Save", Message::Save),
            "Write to /etc/door/greeter.toml (asks for your password)"
        ),
        tip(
            ghost_button("Open in greeter", Message::OpenInGreeter),
            "Launch a full greeter window with these settings"
        ),
        tip(
            ghost_button("Reset", Message::Reset),
            "Reload the saved themes, discarding edits"
        ),
    ]
    .spacing(8);

    // Pin the header, tabs, actions, and status; scroll only the body. (This also
    // keeps the header full-width — when it lived inside the scrollable, the Help
    // tab's scrollbar stole width and clipped the Night toggle.)
    column![
        header,
        tabbar,
        scrollable(body).height(Length::Fill),
        actions,
        text(state.status.clone())
            .size(12)
            .color(c(MUTED.0, MUTED.1, MUTED.2)),
    ]
    .spacing(14)
    .into()
}

/// The Presets tab: load/save whole themes, the live thumbnail gallery, and
/// import/export by path. Lives in its own tab so it doesn't tower over every other
/// section.
fn presets_tab(state: &State) -> Element<'_, Message> {
    let load = group(
        "LOAD",
        column![
            row![
                combo_box(
                    &state.preset_combo,
                    "Search presets…",
                    state.selected_preset.as_ref(),
                    Message::PresetPicked,
                )
                .size(13.0)
                .padding(6)
                .width(Length::Fill),
                ghost_button("🎲 Surprise", Message::Randomize),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            preset_gallery(state),
        ]
        .spacing(10)
        .into(),
    );
    let save = group(
        "SAVE & SHARE",
        column![
            row![
                text_input("name this preset", &state.preset_name)
                    .on_input(Message::PresetNameChanged)
                    .padding(6)
                    .size(13)
                    .style(input_style),
                ghost_button("Save preset", Message::SavePreset),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text_input("~/theme.toml — import / export path", &state.io_path)
                    .on_input(Message::IoPathChanged)
                    .padding(6)
                    .size(13)
                    .style(input_style),
                ghost_button("Import", Message::ImportPreset),
                ghost_button("Export", Message::ExportPreset),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(10)
        .into(),
    );
    column![load, save].spacing(14).into()
}

// ── Per-tab content ─────────────────────────────────────────────────────────

fn colors_tab<'a>(
    state: &'a State,
    pal: &'a Palette,
    card_a: f32,
    h: bool,
) -> Element<'a, Message> {
    let assets = group(
        "LOGO & ASSETS",
        column![
            helped(
                plain_row(
                    "Wallpaper",
                    &pal.wallpaper,
                    "(animated sky)",
                    Param::Wallpaper
                ),
                "Full-screen image; blank uses the animated sky.",
                h
            ),
            helped(
                plain_row("Logo", &pal.logo, "(comet spinner)", Param::Logo),
                "Image shown on the card; blank uses the comet spinner.",
                h
            ),
            helped(
                half_width(color_cell("Logo box", &pal.logo_box, Param::LogoBox)),
                "Backdrop tile behind the logo/spinner. Transparent (alpha 00) = invisible.",
                h
            ),
            helped(
                slider_row(
                    "Box rounding",
                    state.logo_box_radius,
                    0.0..=40.0,
                    1.0,
                    format!("{:.0}px", state.logo_box_radius),
                    Message::LogoBoxRadius
                ),
                "Corner radius of the logo backdrop tile.",
                h
            ),
        ]
        .spacing(9)
        .into(),
    );
    let colors = group(
        "COLORS",
        column![
            row![
                color_cell("BG", &pal.background, Param::Background),
                color_cell("Card", &pal.card, Param::Card)
            ]
            .spacing(10),
            row![
                color_cell("Field", &pal.field, Param::Field),
                color_cell("Accent", &pal.accent, Param::Accent)
            ]
            .spacing(10),
            row![
                color_cell("Text", &pal.foreground, Param::Foreground),
                color_cell("Muted", &pal.muted, Param::Muted)
            ]
            .spacing(10),
            helped(
                half_width(color_cell("Error", &pal.error_color, Param::ErrorColor)),
                "Status-line color when a login fails.",
                h
            ),
            helped(
                slider_row(
                    "Card opacity",
                    card_a,
                    0.0..=1.0,
                    0.01,
                    format!("{}%", (card_a * 100.0).round() as u32),
                    Message::CardAlpha
                ),
                "How see-through the login card is.",
                h
            ),
        ]
        .spacing(9)
        .into(),
    );
    // Colors stays full-width: its cells are already two-up, so a half-width column
    // would quarter each and clip the hex. Assets sits beside the shorter half.
    column![colors, assets].spacing(14).into()
}

fn sky_tab<'a>(state: &'a State, pal: &'a Palette, h: bool) -> Element<'a, Message> {
    let main = group(
        "SKY",
        column![
            helped(
                row![
                    color_label("Scene"),
                    pick_list(
                        &SkyMode::ALL[..],
                        Some(state.sky_mode),
                        Message::SkyModePicked
                    )
                    .text_size(13)
                    .padding(6)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                "Sky scene: auto (day/night by clock), or a fixed scene like aurora.",
                h
            ),
            helped(
                color_cell("Comet color", &pal.comet_color, Param::SkyComet),
                "Color of the comet that drifts across the background.",
                h
            ),
            helped(
                color_cell("Glow color", &pal.glow_color, Param::GlowColor),
                "Tint of the sky glow / daytime sun-haze.",
                h
            ),
            helped(
                slider_row(
                    "Sky glow",
                    pal.sky_glow,
                    0.0..=2.0,
                    0.01,
                    format!("{:.2}", pal.sky_glow),
                    Message::SkyGlow
                ),
                "Strength of that glow (night haze / daytime sun-halo).",
                h
            ),
            helped(
                slider_row(
                    "Star density",
                    state.star_density,
                    0.0..=1.0,
                    0.01,
                    format!("{}%", (state.star_density * 100.0).round() as u32),
                    Message::StarDensity
                ),
                "How many stars fill the night sky.",
                h
            ),
            helped(
                slider_row(
                    "Twinkle",
                    state.star_twinkle,
                    0.0..=4.0,
                    0.1,
                    format!("{:.1}×", state.star_twinkle),
                    Message::StarTwinkle
                ),
                "How fast the stars sparkle.",
                h
            ),
            helped(
                toggle_row(
                    "Background comet",
                    state.comet_enabled,
                    Message::CometEnabled
                ),
                "Show the comet that sweeps across the sky.",
                h
            ),
            helped(
                slider_row(
                    "Comet every",
                    state.comet_interval,
                    3.5..=30.0,
                    0.5,
                    format!("{:.0}s", state.comet_interval),
                    Message::CometInterval
                ),
                "Seconds between comet sweeps.",
                h
            ),
            helped(
                slider_row(
                    "Cloud cover",
                    state.cloud_amount,
                    0.0..=2.0,
                    0.05,
                    format!("{:.0}%", state.cloud_amount * 100.0),
                    Message::CloudAmount
                ),
                "Daytime cloud coverage (day theme only).",
                h
            ),
            helped(
                slider_row(
                    "Cloud drift",
                    state.cloud_speed,
                    0.0..=4.0,
                    0.1,
                    format!("{:.1}×", state.cloud_speed),
                    Message::CloudSpeed
                ),
                "How fast daytime clouds move.",
                h
            ),
        ]
        .spacing(9)
        .into(),
    );
    let sun = group(
        "SUN · DAY",
        column![
            helped(
                slider_row(
                    "Sun X",
                    state.sun_x,
                    0.0..=1.0,
                    0.01,
                    format!("{:.2}", state.sun_x),
                    Message::SunX
                ),
                "Horizontal position of the daytime sun (0 = left).",
                h
            ),
            helped(
                slider_row(
                    "Sun Y",
                    state.sun_y,
                    0.0..=1.0,
                    0.01,
                    format!("{:.2}", state.sun_y),
                    Message::SunY
                ),
                "Vertical position of the daytime sun (0 = top).",
                h
            ),
            helped(
                slider_row(
                    "Sun size",
                    state.sun_size,
                    0.05..=0.6,
                    0.01,
                    format!("{:.2}", state.sun_size),
                    Message::SunSize
                ),
                "Radius of the sun's halo (bigger = wider glow).",
                h
            ),
            helped(
                slider_row(
                    "Sun glow",
                    state.sun_intensity,
                    0.0..=1.0,
                    0.01,
                    format!("{}%", (state.sun_intensity * 100.0).round() as u32),
                    Message::SunIntensity
                ),
                "Brightness of the sun's halo (0 = none).",
                h
            ),
            helped(
                color_cell_with("Sun tint", &state.sun_color, Message::SunColor),
                "Color of the daytime sun's halo.",
                h
            ),
            helped(
                color_cell_with("Cloud lit", &state.cloud_lit, Message::CloudLit),
                "Sun-lit (bright) side of the daytime clouds.",
                h
            ),
            helped(
                color_cell_with("Cloud shade", &state.cloud_shadow, Message::CloudShadow),
                "Shadowed underside of the daytime clouds.",
                h
            ),
        ]
        .spacing(9)
        .into(),
    );
    let advanced = state.expert.then(|| {
        group(
            "SKY · ADVANCED",
            column![
                helped(
                    slider_row(
                        "Glow falloff",
                        state.glow_falloff,
                        0.5..=10.0,
                        0.1,
                        format!("{:.1}", state.glow_falloff),
                        Message::GlowFalloff
                    ),
                    "Tightness of the night sky-glow (higher = smaller).",
                    h
                ),
                helped(
                    slider_row(
                        "Nebula",
                        state.nebula_amount,
                        0.0..=0.5,
                        0.01,
                        format!("{:.2}", state.nebula_amount),
                        Message::NebulaAmount
                    ),
                    "Amount of cloudy nebula haze at night.",
                    h
                ),
                helped(
                    slider_row(
                        "Comet tail",
                        state.comet_tail_decay,
                        2.0..=30.0,
                        0.5,
                        format!("{:.1}", state.comet_tail_decay),
                        Message::CometTailDecay
                    ),
                    "How fast the sky comet's tail fades (higher = shorter).",
                    h
                ),
                helped(
                    slider_row(
                        "Glow X",
                        state.glow_x,
                        0.0..=1.0,
                        0.01,
                        format!("{:.2}", state.glow_x),
                        Message::GlowX
                    ),
                    "Night sky-glow horizontal center (0 = left).",
                    h
                ),
                helped(
                    slider_row(
                        "Glow Y",
                        state.glow_y,
                        0.0..=1.0,
                        0.01,
                        format!("{:.2}", state.glow_y),
                        Message::GlowY
                    ),
                    "Night sky-glow vertical center (0 = top).",
                    h
                ),
                helped(
                    slider_row(
                        "Day haze",
                        state.day_haze,
                        0.0..=1.0,
                        0.01,
                        format!("{}%", (state.day_haze * 100.0).round() as u32),
                        Message::DayHaze
                    ),
                    "Daytime horizon-haze strength.",
                    h
                ),
                helped(
                    slider_row(
                        "Star layers",
                        state.star_layers,
                        1.0..=5.0,
                        1.0,
                        format!("{:.0}", state.star_layers),
                        Message::StarLayers
                    ),
                    "Number of parallax star layers.",
                    h
                ),
                helped(
                    slider_row(
                        "Star size",
                        state.star_size,
                        0.3..=3.0,
                        0.1,
                        format!("{:.1}×", state.star_size),
                        Message::StarSize
                    ),
                    "Size of the stars.",
                    h
                ),
                helped(
                    slider_row(
                        "Nebula drift",
                        state.nebula_speed,
                        0.0..=4.0,
                        0.1,
                        format!("{:.1}×", state.nebula_speed),
                        Message::NebulaSpeed
                    ),
                    "How fast the night nebula drifts.",
                    h
                ),
                helped(
                    slider_row(
                        "Comet tilt",
                        state.comet_tilt,
                        -1.57..=1.57,
                        0.01,
                        format!("{:.2}", state.comet_tilt),
                        Message::CometTilt
                    ),
                    "Rotate the sky comet's path (radians; 0 = default diagonal).",
                    h
                ),
                helped(
                    slider_row(
                        "Comet pause",
                        state.comet_pause,
                        0.0..=6.0,
                        0.1,
                        format!("{:.1}s", state.comet_pause),
                        Message::CometPause
                    ),
                    "Pause after each sky-comet sweep.",
                    h
                ),
                helped(
                    slider_row(
                        "Comet width",
                        state.comet_width,
                        0.3..=3.0,
                        0.1,
                        format!("{:.1}×", state.comet_width),
                        Message::CometWidth
                    ),
                    "Thickness of the sky comet.",
                    h
                ),
                helped(
                    slider_row(
                        "Parallax",
                        state.cursor_parallax,
                        0.0..=3.0,
                        0.1,
                        format!("{:.1}×", state.cursor_parallax),
                        Message::CursorParallax
                    ),
                    "How much the stars drift with the mouse (0 = off).",
                    h
                ),
                helped(
                    slider_row(
                        "Glow pulse",
                        state.glow_pulse,
                        0.0..=2.0,
                        0.05,
                        format!("{:.2}", state.glow_pulse),
                        Message::GlowPulse
                    ),
                    "Gentle breathing of the night sky-glow (0 = steady).",
                    h
                ),
                helped(
                    slider_row(
                        "Grain",
                        state.grain,
                        0.0..=0.3,
                        0.01,
                        format!("{:.2}", state.grain),
                        Message::Grain
                    ),
                    "Film grain over the whole sky (0 = off).",
                    h
                ),
                helped(
                    slider_row(
                        "Vignette",
                        state.vignette,
                        0.0..=1.0,
                        0.01,
                        format!("{}%", (state.vignette * 100.0).round() as u32),
                        Message::Vignette
                    ),
                    "Darken the screen edges (0 = off).",
                    h
                ),
            ]
            .spacing(9)
            .into(),
        )
    });
    let mut col = column![row![main, sun].spacing(16)].spacing(14);
    if let Some(adv) = advanced {
        col = col.push(adv);
    }
    // Per-scene controls: the dials that author the active scene (M8). Shown only when
    // that scene is selected, labeled to read like the scene's blueprint.
    if matches!(state.sky_mode, SkyMode::Synthwave) {
        col = col.push(synthwave_group(state, h));
    }
    col.into()
}

/// The synthwave scene's authoring controls (M8 pilot) — the exact dials behind its
/// grid, sun, and sky. Two columns; only rendered when `sky_mode = synthwave`.
fn synthwave_group(state: &State, h: bool) -> Element<'_, Message> {
    let s = |label: &'static str, v: f32, range: std::ops::RangeInclusive<f32>, step: f32,
             disp: String, on: fn(f32) -> Message| {
        slider_row(label, v, range, step, disp, on)
    };
    group(
        "SYNTHWAVE",
        two_col(vec![
            helped(s("Grid speed", state.synthwave_grid_speed, 0.0..=3.0, 0.02,
                format!("{:.2}", state.synthwave_grid_speed), Message::SwGridSpeed),
                "How fast the grid scrolls toward you.", h),
            helped(s("Grid density", state.synthwave_grid_density, 0.20..=1.00, 0.01,
                format!("{:.2}", state.synthwave_grid_density), Message::SwGridDensity),
                "Spacing of the grid lines (higher = more).", h),
            helped(s("Perspective", state.synthwave_grid_perspective, 0.30..=1.00, 0.01,
                format!("{:.2}", state.synthwave_grid_perspective), Message::SwGridPerspective),
                "How wide the grid spreads toward the front.", h),
            helped(s("Grid glow", state.synthwave_grid_glow, 0.0..=1.40, 0.02,
                format!("{:.2}", state.synthwave_grid_glow), Message::SwGridGlow),
                "Brightness of the neon grid.", h),
            helped(s("Sun size", state.synthwave_sun_size, 0.10..=0.38, 0.004,
                format!("{:.3}", state.synthwave_sun_size), Message::SwSunSize),
                "Radius of the sun on the horizon.", h),
            helped(s("Sun stripes", state.synthwave_sun_stripes, 0.0..=160.0, 2.0,
                format!("{:.0}", state.synthwave_sun_stripes), Message::SwSunStripes),
                "Frequency of the sun's dark bands (0 = solid).", h),
            helped(s("Sun bloom", state.synthwave_sun_bloom, 0.0..=0.70, 0.01,
                format!("{:.2}", state.synthwave_sun_bloom), Message::SwSunBloom),
                "Soft glow around the sun.", h),
            helped(s("Horizon", state.synthwave_horizon, 0.35..=0.75, 0.005,
                format!("{:.2}", state.synthwave_horizon), Message::SwHorizon),
                "Where the ground meets the sky.", h),
            color_cell_with("Grid", &state.synthwave_grid_color, Message::SwGridColor),
            color_cell_with("Sky top", &state.synthwave_sky_top, Message::SwSkyTop),
            color_cell_with("Sky base", &state.synthwave_sky_bottom, Message::SwSkyBottom),
        ]),
    )
}

fn spinner_tab<'a>(state: &'a State, pal: &'a Palette, h: bool) -> Element<'a, Message> {
    let main = group(
        "SPINNER",
        column![
            helped(
                row![
                    color_label("Style"),
                    pick_list(
                        &SpinnerStyle::ALL[..],
                        Some(state.spinner_style),
                        Message::SpinnerStylePicked
                    )
                    .text_size(13)
                    .padding(6)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                "Emblem style (used when no logo image is set).",
                h
            ),
            row![
                color_cell("Comet", &pal.comet, Param::SpinnerComet),
                color_cell("Track", &pal.track, Param::SpinnerTrack)
            ]
            .spacing(10),
            helped(
                slider_row(
                    "Trail",
                    pal.trail,
                    0.15..=1.0,
                    0.01,
                    format!("{}%", (pal.trail * 100.0).round() as u32),
                    Message::SpinnerTrail
                ),
                "Length of the comet's tail.",
                h
            ),
            helped(
                slider_row(
                    "Glow",
                    pal.glow,
                    0.0..=1.0,
                    0.01,
                    format!("{}%", (pal.glow * 100.0).round() as u32),
                    Message::SpinnerGlow
                ),
                "Head bloom (0 = crisp; bands on a light card).",
                h
            ),
            helped(
                slider_row(
                    "Speed",
                    state.spinner_speed,
                    0.0..=6.0,
                    0.1,
                    format!("{:.1}", state.spinner_speed),
                    Message::SpinnerSpeed
                ),
                "Rotation speed.",
                h
            ),
            helped(
                slider_row(
                    "Size",
                    state.spinner_size,
                    24.0..=120.0,
                    1.0,
                    format!("{:.0}px", state.spinner_size),
                    Message::SpinnerSize
                ),
                "Diameter of the card's comet spinner.",
                h
            ),
            helped(
                slider_row(
                    "Pulse",
                    state.spinner_pulse,
                    0.0..=3.0,
                    0.1,
                    format!("{:.1}×", state.spinner_pulse),
                    Message::SpinnerPulse
                ),
                "How fast the spinner head breathes (0 = steady).",
                h
            ),
        ]
        .spacing(9)
        .into(),
    );
    let advanced = state.expert.then(|| {
        group(
            "SPINNER · ADVANCED",
            column![
                helped(
                    slider_row(
                        "Orbit ring",
                        state.spinner_ring,
                        0.0..=0.5,
                        0.01,
                        format!("{:.2}", state.spinner_ring),
                        Message::SpinnerRing
                    ),
                    "Brightness of the spinner's static orbit ring.",
                    h
                ),
                helped(
                    slider_row(
                        "Orbit radius",
                        state.spinner_orbit,
                        0.1..=0.45,
                        0.01,
                        format!("{:.2}", state.spinner_orbit),
                        Message::SpinnerOrbit
                    ),
                    "How far the comet orbits from the spinner center.",
                    h
                ),
            ]
            .spacing(9)
            .into(),
        )
    });
    let mut col = column![main].spacing(14);
    if let Some(adv) = advanced {
        col = col.push(adv);
    }
    col.into()
}

fn card_tab<'a>(state: &'a State, h: bool) -> Element<'a, Message> {
    let g = group(
        "CARD",
        two_col(vec![
            helped(
                row![
                    color_label("Placement"),
                    pick_list(
                        &CardPos::ALL[..],
                        Some(state.card_pos),
                        Message::CardPosPicked
                    )
                    .text_size(13)
                    .padding(6)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                "Where the login card sits on screen.",
                h
            ),
            helped(
                plain_row(
                    "Card rounding",
                    &state.corner_radius,
                    "16",
                    Param::CornerRadius
                ),
                "Corner radius of the login card (px).",
                h
            ),
            helped(
                plain_row("Card width", &state.card_width, "300", Param::CardWidth),
                "Width of the login card (px).",
                h
            ),
            helped(
                slider_row(
                    "Field rounding",
                    state.field_radius,
                    0.0..=30.0,
                    1.0,
                    format!("{:.0}px", state.field_radius),
                    Message::FieldRadius
                ),
                "Corner radius of inputs and buttons (px).",
                h
            ),
            helped(
                slider_row(
                    "Shadow blur",
                    state.card_shadow_blur,
                    0.0..=80.0,
                    1.0,
                    format!("{:.0}px", state.card_shadow_blur),
                    Message::CardShadowBlur
                ),
                "Softness of the card's drop shadow.",
                h
            ),
            helped(
                slider_row(
                    "Shadow strength",
                    state.card_shadow_opacity,
                    0.0..=1.0,
                    0.01,
                    format!("{}%", (state.card_shadow_opacity * 100.0).round() as u32),
                    Message::CardShadowOpacity
                ),
                "Darkness of the card's drop shadow.",
                h
            ),
            helped(
                slider_row(
                    "Accent pulse",
                    state.accent_breathing,
                    0.0..=4.0,
                    0.1,
                    format!("{:.1}×", state.accent_breathing),
                    Message::AccentBreathing
                ),
                "Speed of the card's glowing accent edge (0 = steady).",
                h
            ),
            helped(
                slider_row(
                    "Sheen",
                    state.card_gradient,
                    0.0..=1.0,
                    0.01,
                    format!("{}%", (state.card_gradient * 100.0).round() as u32),
                    Message::CardGradient
                ),
                "Vertical gradient on the card (0 = flat).",
                h
            ),
            helped(
                toggle_row("Backdrop blur", state.card_blur, Message::CardBlur),
                "Frost the sky behind the card (best with a translucent card).",
                h
            ),
        ]),
    );
    column![g].spacing(14).into()
}

fn behavior_tab<'a>(state: &'a State, h: bool) -> Element<'a, Message> {
    let g = group(
        "BEHAVIOR",
        two_col(vec![
            helped(
                plain_row("Font", &state.font, "(stock font)", Param::Font),
                "Installed font family; blank = stock.",
                h
            ),
            helped(
                toggle_row("Clock + date", state.show_clock, Message::ToggleClock),
                "Show the time and date on the card.",
                h
            ),
            helped(
                row![
                    color_label("Clock style"),
                    pick_list(
                        &ClockStyle::ALL[..],
                        Some(state.clock_style),
                        Message::ClockStylePicked
                    )
                    .text_size(13)
                    .padding(6)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                "Digital readout or a drawn analog clock face.",
                h
            ),
            helped(
                toggle_row("24-hour clock", state.clock_24h, Message::Clock24h),
                "Use 24-hour time instead of AM/PM.",
                h
            ),
            helped(
                toggle_row("Show seconds", state.clock_seconds, Message::ClockSeconds),
                "Show seconds on the clock (HH:MM:SS).",
                h
            ),
            helped(
                row![
                    text("Format")
                        .size(13)
                        .width(Length::Fixed(92.0))
                        .color(c(LABEL.0, LABEL.1, LABEL.2)),
                    text_input("e.g. %a %H:%M", &state.clock_format)
                        .on_input(Message::ClockFormat)
                        .padding(6)
                        .size(14)
                        .style(input_style),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                "Custom strftime format; blank = built-in. Ignored for analog.",
                h
            ),
            helped(
                slider_row(
                    "Clock size",
                    state.clock_size,
                    24.0..=120.0,
                    1.0,
                    format!("{:.0}px", state.clock_size),
                    Message::ClockSize
                ),
                "Clock font size.",
                h
            ),
            helped(
                slider_row(
                    "Text scale",
                    state.font_scale,
                    0.7..=1.8,
                    0.05,
                    format!("{:.2}×", state.font_scale),
                    Message::FontScale
                ),
                "Accessibility: scale all card text (1 = default).",
                h
            ),
            helped(
                row![
                    color_label("Font weight"),
                    pick_list(
                        &FontWeight::ALL[..],
                        Some(state.font_weight),
                        Message::FontWeightPicked
                    )
                    .text_size(13)
                    .padding(6)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                "Card text weight (needs the font family to ship that weight).",
                h
            ),
            helped(
                toggle_row("Animate sky", state.animate, Message::ToggleAnimate),
                "Run the stars + comet animation.",
                h
            ),
            helped(
                toggle_row(
                    "Reduced motion",
                    state.reduced_motion,
                    Message::ToggleReducedMotion
                ),
                "Accessibility: still all motion (animation, breathing, parallax, fade).",
                h
            ),
            helped(
                slider_row(
                    "Launch fade",
                    state.fade_ms,
                    0.0..=2000.0,
                    10.0,
                    format!("{:.0}ms", state.fade_ms),
                    Message::FadeMs
                ),
                "Fade-in time when the greeter opens.",
                h
            ),
            helped(
                row![
                    color_label("Day window"),
                    text_input("07:00", &state.day_start)
                        .on_input(|v| Message::Set(Param::DayStart, v))
                        .padding(6)
                        .size(14)
                        .style(input_style),
                    color_label("to"),
                    text_input("19:00", &state.day_end)
                        .on_input(|v| Message::Set(Param::DayEnd, v))
                        .padding(6)
                        .size(14)
                        .style(input_style),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
                "Local times when the day theme is used.",
                h
            ),
        ]),
    );
    column![g].spacing(14).into()
}

/// One tab in the control panel's tab bar — accent-filled when active.
fn tab_button(label: &str, tab: Tab, active: Tab) -> Element<'static, Message> {
    let is_active = tab == active;
    anim_button(text(label.to_string()).size(12))
        .padding([6.0, 8.0])
        .on_press(Message::SelectTab(tab))
        .style(move |_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if is_active {
                    IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.16)
                } else {
                    IColor::TRANSPARENT
                })),
                text_color: if is_active {
                    c(ACCENT.0, ACCENT.1, ACCENT.2)
                } else if hovered {
                    c(FG.0, FG.1, FG.2)
                } else {
                    c(LABEL.0, LABEL.1, LABEL.2)
                },
                border: Border {
                    radius: 9.0.into(),
                    width: if is_active { 1.0 } else { 0.0 },
                    color: IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.30),
                },
                ..Default::default()
            }
        })
        .into()
}

/// A wrapping grid of live preset thumbnails — each a tiny mock of the greeter card
/// in that preset's colors. Clicking one loads it; the active preset gets an accent
/// ring + glow. A grid (vertical scroll via the body) reads far cleaner than a
/// horizontal strip.
fn preset_gallery(state: &State) -> Element<'_, Message> {
    const COLS: usize = 4;
    let mut rows: Vec<Element<Message>> = Vec::new();
    let mut current: Vec<Element<Message>> = Vec::new();
    for p in &state.presets {
        let selected = state.selected_preset.as_ref() == Some(p);
        current.push(preset_thumb(p, selected));
        if current.len() == COLS {
            rows.push(
                Row::with_children(std::mem::take(&mut current))
                    .spacing(10)
                    .into(),
            );
        }
    }
    if !current.is_empty() {
        rows.push(Row::with_children(current).spacing(10).into());
    }
    Column::with_children(rows).spacing(10).into()
}

/// One preset thumbnail: a mini greeter card (background → card → accent button +
/// field bars) in the preset's palette, with its name beneath. A button so a click
/// loads the preset.
fn preset_thumb(p: &Preset, selected: bool) -> Element<'_, Message> {
    const W: f32 = 104.0;
    let sw = p.swatch.unwrap_or(PresetSwatch {
        background: Color { r: 0x1a, g: 0x1b, b: 0x26, a: 0xff },
        card: Color { r: 0x24, g: 0x28, b: 0x3b, a: 0xff },
        field: Color { r: 0x29, g: 0x2e, b: 0x42, a: 0xff },
        accent: Color { r: 0x7a, g: 0xa2, b: 0xf7, a: 0xff },
        foreground: Color { r: 0xc0, g: 0xca, b: 0xf5, a: 0xff },
    });
    let bar = |col: Color, w: f32, h: f32| {
        let fill = col.iced();
        container(Space::new())
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .style(move |_t| container::Style {
                background: Some(Background::Color(fill)),
                border: Border { radius: 2.0.into(), ..Default::default() },
                ..Default::default()
            })
    };
    // The mini card: a couple of field bars + an accent "button", on the card color.
    let card_col = sw.card.iced();
    let mini_card = container(
        column![
            bar(sw.foreground, 26.0, 4.0),
            bar(sw.field, 52.0, 7.0),
            bar(sw.field, 52.0, 7.0),
            bar(sw.accent, 52.0, 9.0),
        ]
        .spacing(4)
        .align_x(Alignment::Center),
    )
    .padding(7)
    .style(move |_t| container::Style {
        background: Some(Background::Color(card_col)),
        border: Border { radius: 6.0.into(), ..Default::default() },
        ..Default::default()
    });

    // The card sits on the preset's background; the whole tile gets the accent ring
    // when selected.
    let bg_col = sw.background.iced();
    let accent = sw.accent.iced();
    let ring = if selected {
        accent
    } else {
        c(0x2a, 0x2e, 0x42)
    };
    let tile = container(mini_card)
        .center_x(Length::Fixed(W))
        .center_y(Length::Fixed(78.0))
        .style(move |_t| container::Style {
            background: Some(Background::Color(bg_col)),
            border: Border {
                radius: 10.0.into(),
                width: if selected { 2.0 } else { 1.0 },
                color: ring,
            },
            // The active preset glows in its own accent so it reads at a glance.
            shadow: if selected {
                Shadow {
                    color: accent.scale_alpha(0.55),
                    offset: Vector::new(0.0, 0.0),
                    blur_radius: 14.0,
                }
            } else {
                Shadow::default()
            },
            ..Default::default()
        });

    let label = text(p.name.clone())
        .size(11)
        .width(Length::Fixed(W))
        .center()
        .color(if selected {
            c(ACCENT.0, ACCENT.1, ACCENT.2)
        } else {
            c(LABEL.0, LABEL.1, LABEL.2)
        });

    anim_button(column![tile, label].spacing(6).align_x(Alignment::Center))
        .padding(5)
        .on_press(Message::PresetPicked(p.clone()))
        // Faint rounded wash on hover so the whole tile feels like one target.
        .style(|_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if hovered {
                    IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.10)
                } else {
                    IColor::TRANSPARENT
                })),
                border: Border {
                    radius: 12.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

/// The Help tab content as Markdown — rendered by iced's `markdown` widget. Explains
/// what each control does and *where it comes from* (Scope Principle 6).
const HELP_MD: &str = r##"# Where it comes from

door's look is plain data — a TOML file the greeter reads at startup.

- **Config:** `/etc/door/greeter.toml` (copied from `/usr/share/door/greeter.toml`). **Save** writes it via `pkexec`; every key is documented in that file.
- **Two palettes:** the greeter runs *before login*, so it can't read your desktop theme — it picks a **night** or **day** palette by the local clock. The Night/Day toggle (top-right) chooses which you're editing.
- **Shared vs day/night:** structural knobs (sizes, speeds, behavior) are shared; colors and a few sky tints are per-variant. Each entry below says which.
- **GPU-drawn:** the sky and comet spinner are real **WGSL** shaders over **wgpu** — the comet is a native port of the `com.genny.tokyonightcomet` Plasma wallpaper. The clock is read from the system clock via `libc` (no date/time crate on the login screen).
- **Presets** are just TOML files in `~/.config/door/presets` (plus the packaged ones). Save / Import / Export read and write them. Edits preview live to the right.

## Colors

The palette. Hex `#rrggbb` or `#rrggbbaa`; click a swatch for the visual picker.

- **Background** · `background` · day/night — solid fill behind everything (and any wallpaper letterbox edges).
- **Card** · `card` · day/night — the login card; alpha makes it translucent glass over the sky.
- **Accent** · `accent` · day/night — focus highlight and the Sign-in button.
- **Foreground / Muted** · `foreground` / `muted` · day/night — primary text; muted is the date, placeholders, idle controls.
- **Error** · `error_color` · day/night — the status line on a failed login (and the Caps-Lock warning).

## Sky

The animated background — a GPU fragment shader.

- **Scene** · `sky_mode` · shared — `auto` (day/night), `seasonal`, or a fixed scene (aurora, storm, rain, snow, meteor, moon, synthwave, fog, plasma, fire, water).
- **Stars** · `star_density` / `star_twinkle` · shared — how many stars and how fast they shimmer.
- **Sky glow** · `sky_glow` / `glow_color` · day/night — atmospheric haze strength and tint (night nebula / daytime sun-haze).
- **Sun** · `sun_x` / `sun_y` / `sun_size` / `sun_intensity` · shared — daytime sun position, halo size, brightness.
- **Grain / Vignette** · `grain` / `vignette` · shared — film grain over the sky and darkened screen edges.

## Spinner

The card emblem when no logo image is set — a GPU shader.

- **Style** · `spinner_style` · shared — comet (door's signature), ring, dots, pulse, or **none** to hide it.
- **Glow / Speed / Trail** · `spinner_glow` / `spinner_speed` / `spinner_trail` · shared — head bloom, rotation speed, trail length.
- **Comet / Track** · `spinner_comet` / `spinner_track` · day/night — the rotating comet color and the static ring it passes over.

## Card

Shape, depth, and the optional backdrop blur.

- **Rounding / Width** · `corner_radius` / `card_width` · shared — card rounding and how wide it sits.
- **Backdrop blur** · `card_blur` · shared — frosted-glass blur of the sky behind the card (a second shader pass).
- **Shadow** · `card_shadow_blur` / `card_shadow_opacity` · shared — drop-shadow softness and darkness.
- **Placement** · `card_pos` · shared — center, left, right, top, or bottom of the screen.

## Behavior

The clock, type, and motion — mostly shared knobs.

- **Clock** · `clock_style` / `clock_format` · shared — digital or a drawn analog face; an optional `strftime` format.
- **Type** · `font` / `font_weight` / `font_scale` · shared — family (must be installed), weight, and an accessibility text-size multiplier.
- **Motion** · `animate` / `reduced_motion` · shared — run the sky animation; reduced-motion stills everything for accessibility.
- **Logo** · `logo` · day/night — an image (SVG drawn crisp, else raster) shown instead of the spinner.
"##;

/// The Help tab: the provenance doc, rendered from Markdown by iced's `markdown`
/// widget. Content lives in [`State::help_md`] (parsed once); links open externally.
fn help_tab(state: &State) -> Element<'_, Message> {
    use iced::widget::markdown;
    let settings = markdown::Settings::with_text_size(
        13,
        markdown::Style::from_palette(iced::Theme::TokyoNight.palette()),
    );
    markdown::view(state.help_md.items(), settings).map(Message::HelpLink)
}

/// reads as a stack of glass tiles, echoing the greeter's frosted card.
fn group<'a>(title: &str, body: Element<'a, Message>) -> Element<'a, Message> {
    let head = row![
        text(title.to_string())
            .size(11)
            .color(c(LABEL.0, LABEL.1, LABEL.2)),
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(hairline),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(column![head, body].spacing(9))
        .width(Length::Fill)
        .padding([10, 13])
        .style(subcard)
        .into()
}

/// Lay a list of controls into two side-by-side columns (first half left, second
/// half right) so a tall section reads as a compact grid instead of a long scroll.
fn two_col<'a>(items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mid = items.len().div_ceil(2);
    let mut left = Column::new().spacing(11);
    let mut right = Column::new().spacing(11);
    for (i, it) in items.into_iter().enumerate() {
        if i < mid {
            left = left.push(it);
        } else {
            right = right.push(it);
        }
    }
    row![
        left.width(Length::Fill),
        right.width(Length::Fill)
    ]
    .spacing(16)
    .into()
}

/// Wrap a widget with a hover tooltip — a small dark chip below it.
fn tip<'a>(content: impl Into<Element<'a, Message>>, label: &'a str) -> Element<'a, Message> {
    tooltip(content, text(label).size(12), tooltip::Position::Bottom)
        .gap(6)
        .padding(8)
        .style(|_t| container::Style {
            background: Some(Background::Color(IColor::from_rgba8(0x12, 0x14, 0x1c, 0.98))),
            border: Border {
                radius: 8.0.into(),
                width: 1.0,
                color: IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.25),
            },
            text_color: Some(c(FG.0, FG.1, FG.2)),
            ..Default::default()
        })
        .into()
}

/// Wrap a control with a one-line description shown only when Help is on.
fn helped<'a>(el: Element<'a, Message>, help: &'a str, help_on: bool) -> Element<'a, Message> {
    if help_on {
        column![el, text(help).size(11).color(c(MUTED.0, MUTED.1, MUTED.2))]
            .spacing(2)
            .into()
    } else {
        el
    }
}

/// A labeled toggle row.
fn toggle_row<'a>(
    label: &'a str,
    value: bool,
    on: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    toggler(value)
        .label(label)
        .on_toggle(on)
        .size(18)
        .text_size(14)
        .into()
}

/// A label · slider · value row — the shared shape for every slider control.
fn slider_row<'a>(
    label: &'a str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    display: String,
    on: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(13)
            .width(Length::Fixed(92.0))
            .color(c(LABEL.0, LABEL.1, LABEL.2)),
        slider(range, value, on).step(step),
        text(display)
            .size(12)
            .width(Length::Fixed(40.0))
            .color(c(MUTED.0, MUTED.1, MUTED.2)),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn subcard(_t: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(IColor::from_rgba8(
            0xff, 0xff, 0xff, 0.022,
        ))),
        border: Border {
            radius: 13.0.into(),
            width: 1.0,
            color: IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.10),
        },
        ..Default::default()
    }
}

fn hairline(_t: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(IColor::from_rgba8(
            0x7a, 0xa2, 0xf7, 0.14,
        ))),
        ..Default::default()
    }
}

fn color_label(label: &str) -> Element<'static, Message> {
    text(label.to_string())
        .size(13)
        .color(c(LABEL.0, LABEL.1, LABEL.2))
        .into()
}

fn plain_row<'a>(
    label: &'a str,
    value: &'a str,
    placeholder: &'a str,
    param: Param,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(13)
            .width(Length::Fixed(92.0))
            .color(c(LABEL.0, LABEL.1, LABEL.2)),
        text_input(placeholder, value)
            .on_input(move |v| Message::Set(param, v))
            .padding(6)
            .size(14)
            .style(input_style),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// A per-variant color cell (routes to the active palette via `Param`).
fn color_cell<'a>(label: &'a str, value: &'a str, param: Param) -> Element<'a, Message> {
    // A param-routed color: hex input plus a swatch button that opens the visual picker.
    row![
        text(label)
            .size(12)
            .width(Length::Fixed(56.0))
            .color(c(LABEL.0, LABEL.1, LABEL.2)),
        text_input("", value)
            .on_input(move |v| Message::Set(param, v))
            .padding(5)
            .size(13)
            .style(input_style),
        swatch_button(value, param),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// Hold a control to roughly half the row, so a lone color cell matches the two-up
/// grid instead of stretching the full panel width.
fn half_width(el: Element<'_, Message>) -> Element<'_, Message> {
    row![el, Space::new().width(Length::Fill)].spacing(10).into()
}

/// The display name of a color [`Param`], for the picker header.
fn param_name(param: Param) -> &'static str {
    match param {
        Param::Background => "Background",
        Param::Card => "Card",
        Param::Field => "Field",
        Param::Accent => "Accent",
        Param::Foreground => "Foreground",
        Param::Muted => "Muted",
        Param::SpinnerComet => "Spinner comet",
        Param::SpinnerTrack => "Spinner track",
        Param::SkyComet => "Sky comet",
        Param::GlowColor => "Glow",
        Param::ErrorColor => "Error",
        Param::LogoBox => "Logo box",
        _ => "Color",
    }
}

/// A color cell bound to an arbitrary message — used for *shared* colors (e.g. the
/// daytime sun tint), which don't live in the per-variant `Palette`/`Param` routing.
fn color_cell_with<'a>(
    label: &'a str,
    value: &'a str,
    on: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(12)
            .width(Length::Fixed(56.0))
            .color(c(LABEL.0, LABEL.1, LABEL.2)),
        text_input("", value)
            .on_input(on)
            .padding(5)
            .size(13)
            .style(input_style),
        swatch(value),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn swatch(value: &str) -> Element<'static, Message> {
    let color = Color::parse(value.trim())
        .map(|col| col.iced())
        .unwrap_or(IColor::TRANSPARENT);
    canvas(SwatchChip { color })
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .into()
}

/// A color chip drawn over a checkerboard so alpha (and near-black) reads honestly —
/// a translucent or dark color is no longer indistinguishable from an empty cell.
struct SwatchChip {
    color: IColor,
}

impl canvas::Program<Message> for SwatchChip {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        use iced::widget::canvas::{Frame, Path, Stroke};
        let mut f = Frame::new(renderer, bounds.size());
        // Checkerboard backdrop.
        let n = 4usize;
        let cell = bounds.width / n as f32;
        let light = IColor::from_rgb8(0x8a, 0x90, 0x9c);
        let dark = IColor::from_rgb8(0x44, 0x49, 0x55);
        for r in 0..n {
            for col in 0..n {
                let shade = if (r + col) % 2 == 0 { light } else { dark };
                f.fill(
                    &Path::rectangle(
                        Point::new(col as f32 * cell, r as f32 * cell),
                        iced::Size::new(cell, cell),
                    ),
                    shade,
                );
            }
        }
        // The color on top (alpha respected), then a hairline border.
        f.fill(&Path::rectangle(Point::new(0.0, 0.0), bounds.size()), self.color);
        f.stroke(
            &Path::rectangle(
                Point::new(0.5, 0.5),
                iced::Size::new(bounds.width - 1.0, bounds.height - 1.0),
            ),
            Stroke::default()
                .with_width(1.0)
                .with_color(IColor::from_rgba8(0x00, 0x00, 0x00, 0.45)),
        );
        vec![f.into_geometry()]
    }
}

/// A swatch that opens the visual HSV picker on click (param-routed colors only).
fn swatch_button(value: &str, param: Param) -> Element<'static, Message> {
    anim_button(swatch(value))
        .padding(0)
        .on_press(Message::OpenPicker(param))
        .style(|_t, _s| button::Style {
            background: None,
            ..Default::default()
        })
        .into()
}

// ── Visual HSV color picker ──────────────────────────────────────────────────

/// `(hue 0–360, saturation 0–1, value 0–1)` from linear-ish 0–1 RGB.
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h.rem_euclid(360.0), s, max)
}

/// 0–1 RGB from `(hue 0–360, saturation 0–1, value 0–1)`.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let cc = v * s;
    let h2 = (h / 60.0).rem_euclid(6.0);
    let x = cc * (1.0 - (h2 % 2.0 - 1.0).abs());
    let (r, g, b) = match h2 as u32 {
        0 => (cc, x, 0.0),
        1 => (x, cc, 0.0),
        2 => (0.0, cc, x),
        3 => (0.0, x, cc),
        4 => (x, 0.0, cc),
        _ => (cc, 0.0, x),
    };
    let m = v - cc;
    (r + m, g + m, b + m)
}

const SV_SIZE: f32 = 200.0;
const HUE_W: f32 = 26.0;
const PICK_GAP: f32 = 14.0;

/// Which sub-region of the picker a drag started in (so it keeps tracking).
#[derive(Clone, Copy, PartialEq)]
enum PickRegion {
    Sv,
    Hue,
}

/// The interactive saturation/value square + hue strip. A pure-drawing-plus-input
/// canvas: it reads the current `(h, s, v)` (baked at construction from the live
/// color) and, on press/drag, publishes a `Set(param, hex)` with the new color
/// (preserving the original alpha). Drag region persists in the canvas state.
struct HsvPicker {
    param: Param,
    h: f32,
    s: f32,
    v: f32,
    a: u8,
}

impl HsvPicker {
    fn hex(&self, h: f32, s: f32, v: f32) -> String {
        let (r, g, b) = hsv_to_rgb(h, s, v);
        Color {
            r: (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            g: (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            b: (b.clamp(0.0, 1.0) * 255.0).round() as u8,
            a: self.a,
        }
        .to_hex()
    }

    /// Map a canvas-local point to a new color hex for the given region.
    fn color_at(&self, region: PickRegion, local: Point) -> String {
        match region {
            PickRegion::Sv => {
                let s = (local.x / SV_SIZE).clamp(0.0, 1.0);
                let v = (1.0 - local.y / SV_SIZE).clamp(0.0, 1.0);
                self.hex(self.h, s, v)
            }
            PickRegion::Hue => {
                let h = (local.y / SV_SIZE).clamp(0.0, 1.0) * 360.0;
                self.hex(h, self.s.max(0.0001), self.v.max(0.0001))
            }
        }
    }
}

impl canvas::Program<Message> for HsvPicker {
    type State = Option<PickRegion>;

    fn update(
        &self,
        drag: &mut Option<PickRegion>,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        use iced::mouse::{Button, Event as Me};
        // Canvas-local cursor (allow tracking slightly outside during a drag).
        let local = cursor
            .position()
            .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
        match event {
            canvas::Event::Mouse(Me::ButtonPressed(Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                let region = if p.x <= SV_SIZE {
                    PickRegion::Sv
                } else if p.x >= SV_SIZE + PICK_GAP {
                    PickRegion::Hue
                } else {
                    return None;
                };
                *drag = Some(region);
                let msg = Message::Set(self.param, self.color_at(region, p));
                Some(canvas::Action::publish(msg).and_capture())
            }
            canvas::Event::Mouse(Me::CursorMoved { .. }) => {
                let region = (*drag)?;
                let local = local?;
                let msg = Message::Set(self.param, self.color_at(region, local));
                Some(canvas::Action::publish(msg).and_capture())
            }
            canvas::Event::Mouse(Me::ButtonReleased(Button::Left)) => {
                if drag.take().is_some() {
                    Some(canvas::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _drag: &Option<PickRegion>,
        renderer: &Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        use iced::widget::canvas::{gradient, Frame, Path, Stroke};
        let mut frame = Frame::new(renderer, bounds.size());
        let white = IColor::WHITE;
        let black = IColor::BLACK;

        // ── Saturation/Value square: pure hue, then white→clear (sat) and clear→black (val).
        let (hr, hg, hb) = hsv_to_rgb(self.h, 1.0, 1.0);
        let hue_col = IColor::from_rgb(hr, hg, hb);
        let sv = Path::rectangle(Point::new(0.0, 0.0), iced::Size::new(SV_SIZE, SV_SIZE));
        frame.fill(&sv, hue_col);
        frame.fill(
            &sv,
            gradient::Linear::new(Point::new(0.0, 0.0), Point::new(SV_SIZE, 0.0))
                .add_stop(0.0, white)
                .add_stop(1.0, IColor { a: 0.0, ..white }),
        );
        frame.fill(
            &sv,
            gradient::Linear::new(Point::new(0.0, 0.0), Point::new(0.0, SV_SIZE))
                .add_stop(0.0, IColor { a: 0.0, ..black })
                .add_stop(1.0, black),
        );
        // SV cursor ring (white over black for contrast on any background).
        let sp = Point::new(self.s * SV_SIZE, (1.0 - self.v) * SV_SIZE);
        frame.stroke(
            &Path::circle(sp, 6.0),
            Stroke::default().with_width(2.0).with_color(white),
        );
        frame.stroke(
            &Path::circle(sp, 7.5),
            Stroke::default().with_width(1.0).with_color(black),
        );

        // ── Hue strip: a vertical rainbow.
        let hx = SV_SIZE + PICK_GAP;
        let strip = Path::rectangle(Point::new(hx, 0.0), iced::Size::new(HUE_W, SV_SIZE));
        let mut grad = gradient::Linear::new(Point::new(hx, 0.0), Point::new(hx, SV_SIZE));
        for i in 0..=6 {
            let (r, g, b) = hsv_to_rgb(i as f32 * 60.0, 1.0, 1.0);
            grad = grad.add_stop(i as f32 / 6.0, IColor::from_rgb(r, g, b));
        }
        frame.fill(&strip, grad);
        // Hue cursor: a horizontal bar at the current hue.
        let hy = self.h / 360.0 * SV_SIZE;
        frame.stroke(
            &Path::rectangle(Point::new(hx - 2.0, hy - 2.0), iced::Size::new(HUE_W + 4.0, 4.0)),
            Stroke::default().with_width(2.0).with_color(white),
        );

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _drag: &Option<PickRegion>,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::Crosshair
    }
}

/// The centered HSV picker overlay (a dim backdrop + a glass panel), shown when a
/// swatch is being edited. Built over the whole window via the top-level stack.
fn picker_overlay(state: &State, param: Param) -> Element<'_, Message> {
    let hex = state.color_hex(param);
    let cur = Color::parse(hex.trim()).unwrap_or(Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    });
    let (h, s, v) = rgb_to_hsv(
        cur.r as f32 / 255.0,
        cur.g as f32 / 255.0,
        cur.b as f32 / 255.0,
    );
    let picker = HsvPicker {
        param,
        h,
        s,
        v,
        a: cur.a,
    };

    let panel = container(
        column![
            row![
                text(param_name(param))
                    .size(14)
                    .color(c(FG.0, FG.1, FG.2)),
                Space::new().width(Length::Fill),
                ghost_button("Done", Message::ClosePicker),
            ]
            .align_y(Alignment::Center),
            canvas(picker)
                .width(Length::Fixed(SV_SIZE + PICK_GAP + HUE_W))
                .height(Length::Fixed(SV_SIZE)),
            color_cell("Hex", hex, param),
        ]
        .spacing(12)
        // Pin the popup to the picker's own width — without this the Fill-width hex
        // cell expands the whole panel to span the window.
        .width(Length::Fixed(SV_SIZE + PICK_GAP + HUE_W)),
    )
    .padding(18)
    .style(glass_panel);

    // Dim backdrop (click to dismiss) with the panel centered on top.
    let backdrop = button(Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .on_press(Message::ClosePicker)
        .style(|_t, _s| button::Style {
            background: Some(Background::Color(IColor::from_rgba8(0, 0, 0, 0.5))),
            ..Default::default()
        });
    iced::widget::stack![
        backdrop,
        container(panel).center_x(Length::Fill).center_y(Length::Fill),
    ]
    .into()
}

fn input_style(_t: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    let mut selection = c(ACCENT.0, ACCENT.1, ACCENT.2);
    selection.a = 0.30;
    text_input::Style {
        background: Background::Color(c(0x1d, 0x20, 0x30)),
        border: Border {
            radius: 9.0.into(),
            width: 1.0,
            color: if focused {
                c(ACCENT.0, ACCENT.1, ACCENT.2)
            } else {
                c(0x2a, 0x2e, 0x42)
            },
        },
        icon: c(MUTED.0, MUTED.1, MUTED.2),
        placeholder: c(MUTED.0, MUTED.1, MUTED.2),
        value: c(FG.0, FG.1, FG.2),
        selection,
    }
}

fn primary_button(label: &str, msg: Message) -> Element<'_, Message> {
    anim_button(text(label.to_string()).size(14).color(c(0x16, 0x16, 0x1e)))
        .padding(9)
        .on_press(msg)
        .style(|_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if hovered {
                    c(0x9a, 0xbb, 0xff)
                } else {
                    c(ACCENT.0, ACCENT.1, ACCENT.2)
                })),
                text_color: c(0x16, 0x16, 0x1e),
                border: Border {
                    radius: 9.0.into(),
                    ..Default::default()
                },
                // The button lifts on hover — a soft accent-tinted shadow that grows.
                shadow: Shadow {
                    color: IColor::from_rgba8(0x7a, 0xa2, 0xf7, if hovered { 0.5 } else { 0.28 }),
                    offset: Vector::new(0.0, if hovered { 3.0 } else { 1.0 }),
                    blur_radius: if hovered { 16.0 } else { 6.0 },
                },
                ..Default::default()
            }
        })
        .into()
}

fn ghost_button(label: &str, msg: Message) -> Element<'_, Message> {
    anim_button(text(label.to_string()).size(14))
        .padding(9)
        .on_press(msg)
        .style(|_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                // A faint accent wash fills in on hover (instead of just recoloring text).
                background: Some(Background::Color(if hovered {
                    IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.12)
                } else {
                    IColor::TRANSPARENT
                })),
                text_color: if hovered {
                    c(ACCENT.0, ACCENT.1, ACCENT.2)
                } else {
                    c(LABEL.0, LABEL.1, LABEL.2)
                },
                border: Border {
                    radius: 9.0.into(),
                    width: 1.0,
                    color: if hovered {
                        IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.45)
                    } else {
                        c(0x2a, 0x2e, 0x42)
                    },
                },
                ..Default::default()
            }
        })
        .into()
}

/// Card fill: flat, or a subtle top-lit vertical gradient when `gradient` > 0
/// (mirrors door-greeter's `card_background`).
fn card_bg(card: IColor, gradient: f32) -> Background {
    if gradient <= 0.0 {
        return Background::Color(card);
    }
    let amt = 0.18 * gradient.clamp(0.0, 1.0);
    let lit = |x: f32| x + (1.0 - x) * amt;
    let top = IColor {
        r: lit(card.r),
        g: lit(card.g),
        b: lit(card.b),
        a: card.a,
    };
    Background::Gradient(iced::Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, top)
            .add_stop(1.0, card),
    ))
}

/// Whether a logo path is an SVG (case-insensitive `.svg`) — vector vs raster.
fn is_svg(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

/// Format the current local time with a `strftime` string, for the preview (mirrors
/// the greeter's own formatting). Empty on a bad format → the caller shows a mock.
fn sample_strftime(fmt: &str) -> String {
    let Ok(cfmt) = std::ffi::CString::new(fmt.trim()) else {
        return String::new();
    };
    // SAFETY: localtime_r fills our owned tm; strftime writes ≤ buf.len() bytes into
    // buf and reads only the tm and the NUL-terminated format.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        let mut buf = [0u8; 128];
        let n = libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            cfmt.as_ptr(),
            &tm,
        );
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

/// A non-interactive mock of the greeter card, themed from the draft.
fn preview_card(t: &Theme, anim: f32) -> Element<'static, Message> {
    let fg = t.foreground.iced();
    let muted = t.muted.iced();
    // Preview the chosen weight on the card's default-family text (the greeter's own
    // family isn't loaded here, but the weight reads).
    let pfont = iced::Font {
        weight: t.font_weight.iced(),
        ..iced::Font::DEFAULT
    };

    let header: Element<Message> = if t.show_clock {
        let time_widget: Element<Message> = match t.clock_style {
            ClockStyle::Digital => {
                // A custom strftime format previews against the real clock; otherwise a
                // representative mock so the size/placement read without a live tick.
                let label = match t.clock_format.as_deref() {
                    Some(fmt) if !fmt.trim().is_empty() => sample_strftime(fmt),
                    _ if t.clock_seconds => "12:34:56".to_string(),
                    _ => "12:34".to_string(),
                };
                text(label)
                    .size(t.clock_size * t.font_scale)
                    .font(pfont)
                    .color(fg)
                    .into()
            }
            ClockStyle::Analog => {
                // Preview pose: 12:34 (matching the digital mock) with the second
                // hand sweeping live off the preview's animation clock.
                let tau = std::f32::consts::TAU;
                let sec = anim % 60.0;
                let minute = 34.0 + sec / 60.0;
                let hour = minute / 60.0;
                let angles = (hour / 12.0 * tau, minute / 60.0 * tau, sec / 60.0 * tau);
                let d = t.clock_size * t.font_scale * 2.3;
                canvas(AnalogClock::new(t, 1.0, angles))
                    .width(Length::Fixed(d))
                    .height(Length::Fixed(d))
                    .into()
            }
        };
        column![
            time_widget,
            text("Friday, June 27")
                .size(13.0 * t.font_scale)
                .color(muted),
        ]
        .spacing(2)
        .align_x(Alignment::Center)
        .into()
    } else {
        Space::new().into()
    };

    let emblem: Option<Element<Message>> = match &t.logo {
        Some(path) if is_svg(path) => Some(
            svg(svg::Handle::from_path(path.clone()))
                .height(Length::Fixed(56.0))
                .into(),
        ),
        Some(path) => Some(image(image::Handle::from_path(path)).height(Length::Fixed(56.0)).into()),
        None if t.spinner_style.is_hidden() => None,
        None => Some(
            shader(SpinnerShader::from_theme(t, anim, 1.0))
                .width(Length::Fixed(t.spinner_size))
                .height(Length::Fixed(t.spinner_size))
                .into(),
        ),
    };
    let logo_bg = t.logo_box.iced();
    let logo_radius = t.logo_box_radius;
    let logo: Option<Element<Message>> = emblem.map(|e| {
        container(e)
            .padding(6)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(logo_bg)),
                border: Border {
                    radius: logo_radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    });

    let field = |placeholder: &'static str, t: &Theme| {
        let muted = t.muted.iced();
        let field = t.field;
        container(text(placeholder).size(14.0 * t.font_scale).color(muted))
            .width(Length::Fill)
            .padding(11)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(field.iced())),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
    };

    let accent = t.accent;
    let sign_in = container(
        text("Sign in")
            .size(15.0 * t.font_scale)
            .width(Length::Fill)
            .center()
            .color(iced::Color::from_rgb8(0x16, 0x16, 0x1e)),
    )
    .width(Length::Fill)
    .padding(11)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(accent.iced())),
        border: Border {
            radius: 10.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let mut body_items: Vec<Element<Message>> = vec![header];
    if let Some(logo) = logo {
        body_items.push(logo);
    }
    body_items.push(field("user", t).into());
    body_items.push(field("password", t).into());
    body_items.push(sign_in.into());
    let body = Column::with_children(body_items)
        .spacing(12)
        .align_x(Alignment::Center);

    let card = t.card;
    let accent = t.accent;
    let radius = t.corner_radius;
    let gradient = t.card_gradient;
    container(body)
        .padding(26)
        .width(Length::Fixed(t.card_width))
        .style(move |_theme| container::Style {
            background: Some(card_bg(card.iced(), gradient)),
            border: Border {
                radius: radius.into(),
                width: 1.0,
                color: accent.iced_alpha(0.28),
            },
            shadow: Shadow {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                offset: Vector::new(0.0, 12.0),
                blur_radius: 38.0,
            },
            ..Default::default()
        })
        .into()
}

/// The frosted control panel.
fn glass_panel(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(iced::Color::from_rgba8(
            0x0e, 0x0f, 0x16, 0.74,
        ))),
        border: Border {
            radius: 18.0.into(),
            width: 1.0,
            color: iced::Color::from_rgba8(0x7a, 0xa2, 0xf7, 0.20),
        },
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: Vector::new(6.0, 0.0),
            blur_radius: 30.0,
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{expand_tilde, hsv_to_rgb, rgb_to_hsv, State};
    use door_theme::Theme;

    /// Export → import round-trips a theme pair through a file (the glue behind the
    /// path-field Import/Export buttons): render_pair to disk, parse_pair back equal.
    #[test]
    fn export_import_round_trips() {
        let night = Theme::default();
        let mut day = Theme::default();
        day.accent = door_theme::Color::parse("#ffcc00").unwrap();
        let body = Theme::render_pair(&night, &day, "07:00", "19:00");
        let path = std::env::temp_dir().join("door-settings-io-test.toml");
        std::fs::write(&path, &body).unwrap();
        let read = std::fs::read_to_string(&path).unwrap();
        let (n2, d2, (start, end)) = Theme::parse_pair(&read).unwrap();
        assert_eq!(n2.accent, night.accent);
        assert_eq!(d2.accent, day.accent);
        assert_eq!((start, end), (7 * 60, 19 * 60));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tilde_expands_to_home() {
        // SAFETY: single-threaded test setup.
        unsafe { std::env::set_var("HOME", "/home/tester") };
        assert_eq!(
            expand_tilde("~/theme.toml"),
            std::path::PathBuf::from("/home/tester/theme.toml")
        );
        assert_eq!(
            expand_tilde("/abs/path.toml"),
            std::path::PathBuf::from("/abs/path.toml")
        );
    }

    /// HSV ↔ RGB round-trips within rounding for a spread of saturated/dim colors —
    /// the math behind the visual picker's cursor placement and emitted hex.
    #[test]
    fn hsv_rgb_round_trips() {
        let cases = [
            (0.48, 0.64, 0.97), // accent blue
            (1.0, 0.0, 0.0),    // pure red
            (0.0, 1.0, 0.0),    // pure green
            (0.10, 0.10, 0.12), // near-black card
            (0.77, 0.81, 0.96), // light foreground
        ];
        for (r, g, b) in cases {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert!((r - r2).abs() < 1e-4, "r {r} -> {r2}");
            assert!((g - g2).abs() < 1e-4, "g {g} -> {g2}");
            assert!((b - b2).abs() < 1e-4, "b {b} -> {b2}");
        }
    }

    /// Every shipped preset parses, applies, and builds both variants without
    /// panicking — the path the dice button (Randomize) exercises at runtime.
    #[test]
    fn all_presets_load_and_build() {
        for dir in ["dist/door/presets", "/usr/share/door/presets"] {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("toml") {
                    continue;
                }
                let body = std::fs::read_to_string(&path).unwrap();
                let (night, day, window) = door_theme::Theme::parse_pair(&body)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                let mut s = State::new();
                s.apply_pair(&night, &day, window);
                s.rebuild_preview();
                let _ = s.build(false).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                let _ = s.build(true).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            }
        }
    }
}
