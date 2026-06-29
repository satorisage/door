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
    button, column, container, image, pick_list, row, scrollable, shader, slider, text, text_input,
    toggler, Space,
};
use iced::{
    Alignment, Background, Border, Color as IColor, ContentFit, Element, Length, Shadow,
    Subscription, Task, Vector,
};

use door_theme::skyshader::{SkyShader, SpinnerShader};
use door_theme::{CardPos, Color, SkyMode, Theme};

fn main() -> iced::Result {
    iced::application(State::new, update, view)
        .title("door — greeter settings")
        .style(app_style)
        .window_size((1180.0, 940.0))
        .subscription(subscription)
        .run()
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
    SpinnerOrbit(f32),
    // Spinner
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
    spinner_orbit: f32,
    spinner_size: f32,
    spinner_pulse: f32,
    card_shadow_blur: f32,
    card_shadow_opacity: f32,
    accent_breathing: f32,
    field_radius: f32,
    logo_box_radius: f32,
    clock_24h: bool,
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
    selected_preset: Option<Preset>,
    preset_name: String,
    // Which variant is being edited / previewed.
    editing_day: bool,
    // Which control tab is showing.
    tab: Tab,
    status: String,
    anim: f32,
    started: Instant,
    /// Cached built theme for the preview/window — rebuilt on edits, not per frame
    /// (parsing every color twice each vsync frame was the day-flip stutter).
    preview: Theme,
}

fn minutes_to_hhmm(m: u32) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// A saved theme preset (both variants + window), backed by a `.toml` file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Preset {
    name: String,
    path: PathBuf,
}

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
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
                out.push(Preset { name, path });
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
            spinner_orbit: night.spinner_orbit,
            spinner_size: night.spinner_size,
            spinner_pulse: night.spinner_pulse,
            card_shadow_blur: night.card_shadow_blur,
            card_shadow_opacity: night.card_shadow_opacity,
            accent_breathing: night.accent_breathing,
            field_radius: night.field_radius,
            logo_box_radius: night.logo_box_radius,
            clock_24h: night.clock_24h,
            fade_ms: night.fade_ms,
            glow_falloff: night.glow_falloff,
            nebula_amount: night.nebula_amount,
            comet_tail_decay: night.comet_tail_decay,
            spinner_ring: night.spinner_ring,
            expert: std::env::args().any(|a| a == "--expert"),
            help_on: false,
            presets: scan_presets(),
            selected_preset: None,
            preset_name: String::new(),
            // Dev: start on the day variant when DOOR_SETTINGS_DAY is set.
            editing_day: std::env::var_os("DOOR_SETTINGS_DAY").is_some(),
            tab: Tab::Colors,
            status: "Loaded night + day themes.".to_string(),
            anim: 0.0,
            started: Instant::now(),
            preview: Theme::default(),
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
        self.spinner_orbit = night.spinner_orbit;
        self.spinner_size = night.spinner_size;
        self.spinner_pulse = night.spinner_pulse;
        self.card_shadow_blur = night.card_shadow_blur;
        self.card_shadow_opacity = night.card_shadow_opacity;
        self.accent_breathing = night.accent_breathing;
        self.field_radius = night.field_radius;
        self.logo_box_radius = night.logo_box_radius;
        self.clock_24h = night.clock_24h;
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
            spinner_orbit: self.spinner_orbit,
            spinner_size: self.spinner_size,
            spinner_pulse: self.spinner_pulse,
            card_shadow_blur: self.card_shadow_blur,
            card_shadow_opacity: self.card_shadow_opacity,
            accent_breathing: self.accent_breathing,
            field_radius: self.field_radius,
            clock_24h: self.clock_24h,
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
        Message::SpinnerOrbit(v) => state.spinner_orbit = v.clamp(0.1, 0.45),
        Message::SpinnerSize(v) => state.spinner_size = v.clamp(24.0, 120.0),
        Message::SpinnerPulse(v) => state.spinner_pulse = v.clamp(0.0, 3.0),
        Message::CardShadowBlur(v) => state.card_shadow_blur = v.clamp(0.0, 80.0),
        Message::CardShadowOpacity(v) => state.card_shadow_opacity = v.clamp(0.0, 1.0),
        Message::AccentBreathing(v) => state.accent_breathing = v.clamp(0.0, 4.0),
        Message::FieldRadius(v) => state.field_radius = v.clamp(0.0, 30.0),
        Message::LogoBoxRadius(v) => state.logo_box_radius = v.clamp(0.0, 40.0),
        Message::Clock24h(on) => state.clock_24h = on,
        Message::FadeMs(v) => state.fade_ms = v.clamp(0.0, 2000.0),
        // Expert.
        Message::GlowFalloff(v) => state.glow_falloff = v.clamp(0.5, 10.0),
        Message::NebulaAmount(v) => state.nebula_amount = v.clamp(0.0, 0.5),
        Message::CometTailDecay(v) => state.comet_tail_decay = v.clamp(2.0, 30.0),
        Message::SpinnerRing(v) => state.spinner_ring = v.clamp(0.0, 0.5),
        Message::Tick => state.anim = state.started.elapsed().as_secs_f32() % 10_000.0,
        Message::Reset => {
            let keep = state.editing_day;
            *state = State::new();
            state.editing_day = keep;
            state.status = "Reloaded the saved themes.".to_string();
        }
        Message::PresetNameChanged(s) => state.preset_name = s,
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

    let panel = container(scrollable(controls(state)))
        .width(Length::Fixed(372.0))
        .height(Length::Fill)
        .padding(16)
        .style(glass_panel);
    let left = container(panel).padding(16);

    let pv = container(preview_card(theme, state.anim)).padding(24);
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
        Space::new().into()
    };

    match &theme.wallpaper {
        Some(path) => {
            let bg = image(image::Handle::from_path(path))
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Cover);
            iced::widget::stack![bg, sky_layer, content].into()
        }
        None => iced::widget::stack![sky_layer, content].into(),
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

    // Header: title + the Night/Day variant toggle, a subtitle, and the Help/Advanced
    // toggles (Advanced reveals the expert controls; it's also seeded by `--expert`).
    let header = column![
        row![
            text("Greeter").size(26).color(c(FG.0, FG.1, FG.2)),
            Space::new().width(Length::Fill),
            toggler(editing_day)
                .label(if editing_day { "Day" } else { "Night" })
                .on_toggle(Message::EditDay)
                .size(18)
                .text_size(13),
        ]
        .align_y(Alignment::Center),
        text("Edits preview live · Save asks for your password")
            .size(12)
            .color(c(MUTED.0, MUTED.1, MUTED.2)),
        row![
            toggler(state.help_on)
                .label("Help")
                .on_toggle(Message::ToggleHelp)
                .size(16)
                .text_size(12),
            Space::new().width(Length::Fill),
            toggler(state.expert)
                .label("Advanced")
                .on_toggle(Message::ToggleExpert)
                .size(16)
                .text_size(12),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(6);

    let tabbar = row![
        tab_button("Colors", Tab::Colors, state.tab),
        tab_button("Sky", Tab::Sky, state.tab),
        tab_button("Spinner", Tab::Spinner, state.tab),
        tab_button("Card", Tab::Card, state.tab),
        tab_button("Behavior", Tab::Behavior, state.tab),
    ]
    .spacing(5);

    let body: Element<Message> = match state.tab {
        Tab::Colors => colors_tab(state, pal, card_a, h),
        Tab::Sky => sky_tab(state, pal, h),
        Tab::Spinner => spinner_tab(state, pal, h),
        Tab::Card => card_tab(state, h),
        Tab::Behavior => behavior_tab(state, h),
    };

    let actions = row![
        primary_button("Save", Message::Save),
        ghost_button("Open in greeter", Message::OpenInGreeter),
        ghost_button("Reset", Message::Reset),
    ]
    .spacing(8);

    // Presets: load a saved day+night theme, or save the current one by name.
    let presets = group(
        "PRESETS",
        column![
            pick_list(
                &state.presets[..],
                state.selected_preset.clone(),
                Message::PresetPicked,
            )
            .placeholder("Load a preset…")
            .text_size(13)
            .padding(6)
            .width(Length::Fill),
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
        ]
        .spacing(8)
        .into(),
    );

    column![
        header,
        presets,
        tabbar,
        body,
        actions,
        text(state.status.clone())
            .size(12)
            .color(c(MUTED.0, MUTED.1, MUTED.2)),
    ]
    .spacing(14)
    .into()
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
                color_cell("Logo box", &pal.logo_box, Param::LogoBox),
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
                color_cell("Error", &pal.error_color, Param::ErrorColor),
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
    column![assets, colors].spacing(14).into()
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
            ]
            .spacing(9)
            .into(),
        )
    });
    let mut col = column![main, sun].spacing(14);
    if let Some(adv) = advanced {
        col = col.push(adv);
    }
    col.into()
}

fn spinner_tab<'a>(state: &'a State, pal: &'a Palette, h: bool) -> Element<'a, Message> {
    let main = group(
        "SPINNER",
        column![
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
        column![
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
        ]
        .spacing(9)
        .into(),
    );
    column![g].spacing(14).into()
}

fn behavior_tab<'a>(state: &'a State, h: bool) -> Element<'a, Message> {
    let g = group(
        "BEHAVIOR",
        column![
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
                toggle_row("24-hour clock", state.clock_24h, Message::Clock24h),
                "Use 24-hour time instead of AM/PM.",
                h
            ),
            helped(
                toggle_row("Animate sky", state.animate, Message::ToggleAnimate),
                "Run the stars + comet animation.",
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
        ]
        .spacing(9)
        .into(),
    );
    column![g].spacing(14).into()
}

/// One tab in the control panel's tab bar — accent-filled when active.
fn tab_button(label: &str, tab: Tab, active: Tab) -> Element<'static, Message> {
    let is_active = tab == active;
    button(text(label.to_string()).size(12))
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

/// A titled, faintly-bordered sub-card grouping one section's controls — the panel
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

    container(column![head, body].spacing(11))
        .width(Length::Fill)
        .padding(14)
        .style(subcard)
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
    color_cell_with(label, value, move |v| Message::Set(param, v))
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
            .width(Length::Fixed(44.0))
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
    let fill = Color::parse(value.trim()).map(|col| Background::Color(col.iced()));
    container(Space::new())
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .style(move |_t| container::Style {
            background: fill,
            border: Border {
                radius: 7.0.into(),
                width: 1.0,
                color: c(0x2a, 0x2e, 0x42),
            },
            ..Default::default()
        })
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
    button(text(label.to_string()).size(14).color(c(0x16, 0x16, 0x1e)))
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
                ..Default::default()
            }
        })
        .into()
}

fn ghost_button(label: &str, msg: Message) -> Element<'_, Message> {
    button(text(label.to_string()).size(14))
        .padding(9)
        .on_press(msg)
        .style(|_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(IColor::TRANSPARENT)),
                text_color: if hovered {
                    c(ACCENT.0, ACCENT.1, ACCENT.2)
                } else {
                    c(LABEL.0, LABEL.1, LABEL.2)
                },
                border: Border {
                    radius: 9.0.into(),
                    width: 1.0,
                    color: c(0x2a, 0x2e, 0x42),
                },
                ..Default::default()
            }
        })
        .into()
}

/// A non-interactive mock of the greeter card, themed from the draft.
fn preview_card(t: &Theme, anim: f32) -> Element<'static, Message> {
    let fg = t.foreground.iced();
    let muted = t.muted.iced();

    let header: Element<Message> = if t.show_clock {
        column![
            text("12:34").size(54).color(fg),
            text("Friday, June 27").size(13).color(muted),
        ]
        .spacing(2)
        .align_x(Alignment::Center)
        .into()
    } else {
        Space::new().into()
    };

    let logo: Element<Message> = match &t.logo {
        Some(path) => image(image::Handle::from_path(path))
            .height(Length::Fixed(56.0))
            .into(),
        None => shader(SpinnerShader::from_theme(t, anim, 1.0))
            .width(Length::Fixed(t.spinner_size))
            .height(Length::Fixed(t.spinner_size))
            .into(),
    };
    let logo_bg = t.logo_box.iced();
    let logo_radius = t.logo_box_radius;
    let logo: Element<Message> = container(logo)
        .padding(6)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(logo_bg)),
            border: Border {
                radius: logo_radius.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

    let field = |placeholder: &'static str, t: &Theme| {
        let muted = t.muted.iced();
        let field = t.field;
        container(text(placeholder).size(14).color(muted))
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
            .size(15)
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

    let body = column![
        header,
        logo,
        field("user", t),
        field("password", t),
        sign_in
    ]
    .spacing(12)
    .align_x(Alignment::Center);

    let card = t.card;
    let accent = t.accent;
    let radius = t.corner_radius;
    container(body)
        .padding(26)
        .width(Length::Fixed(t.card_width))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(card.iced())),
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
