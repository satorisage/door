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
    button, column, container, image, row, scrollable, shader, slider, text, text_input,
    toggler, Space,
};
use iced::{
    Alignment, Background, Border, Color as IColor, ContentFit, Element, Length, Shadow,
    Subscription, Task, Vector,
};

use door_theme::skyshader::{SkyShader, SpinnerShader};
use door_theme::{Color, Theme};

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
    // Per-variant sky glow strength + login-error color.
    sky_glow: f32,
    error_color: String,
}

impl Palette {
    fn from_theme(t: &Theme) -> Self {
        let path = |p: &Option<PathBuf>| {
            p.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
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
            sky_glow: t.sky_glow,
            error_color: t.error_color.to_hex(),
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
    ErrorColor,
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
    SelectTab(Tab),
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
    // Spinner
    SpinnerSize(f32),
    SpinnerPulse(f32),
    // Card
    CardShadowBlur(f32),
    CardShadowOpacity(f32),
    AccentBreathing(f32),
    FieldRadius(f32),
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
}

struct State {
    night: Palette,
    day: Palette,
    // Shared structural keys.
    font: String,
    corner_radius: String,
    card_width: String,
    day_start: String,
    day_end: String,
    spinner_speed: f32,
    show_clock: bool,
    animate: bool,
    // Shared sky / spinner / card / behavior controls (Tier 1+2).
    star_density: f32,
    star_twinkle: f32,
    comet_enabled: bool,
    comet_interval: f32,
    cloud_amount: f32,
    cloud_speed: f32,
    spinner_size: f32,
    spinner_pulse: f32,
    card_shadow_blur: f32,
    card_shadow_opacity: f32,
    accent_breathing: f32,
    field_radius: f32,
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

impl State {
    fn new() -> Self {
        let (night, day, (start, end)) = Theme::load_pair();
        let mut s = State {
            night: Palette::from_theme(&night),
            day: Palette::from_theme(&day),
            font: night.font.clone().unwrap_or_default(),
            corner_radius: night.corner_radius.to_string(),
            card_width: night.card_width.to_string(),
            day_start: minutes_to_hhmm(start),
            day_end: minutes_to_hhmm(end),
            spinner_speed: night.spinner_speed,
            show_clock: night.show_clock,
            animate: night.animate,
            star_density: night.star_density,
            star_twinkle: night.star_twinkle,
            comet_enabled: night.comet_enabled,
            comet_interval: night.comet_interval,
            cloud_amount: night.cloud_amount,
            cloud_speed: night.cloud_speed,
            spinner_size: night.spinner_size,
            spinner_pulse: night.spinner_pulse,
            card_shadow_blur: night.card_shadow_blur,
            card_shadow_opacity: night.card_shadow_opacity,
            accent_breathing: night.accent_breathing,
            field_radius: night.field_radius,
            clock_24h: night.clock_24h,
            fade_ms: night.fade_ms,
            glow_falloff: night.glow_falloff,
            nebula_amount: night.nebula_amount,
            comet_tail_decay: night.comet_tail_decay,
            spinner_ring: night.spinner_ring,
            expert: std::env::args().any(|a| a == "--expert"),
            help_on: false,
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
        if self.editing_day { &self.day } else { &self.night }
    }

    fn set(&mut self, param: Param, value: String) {
        let pal = if self.editing_day { &mut self.day } else { &mut self.night };
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
            Param::ErrorColor => pal.error_color = value,
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
            show_clock: self.show_clock,
            animate: self.animate,
            is_day: day,
            spinner_glow: pal.glow,
            spinner_speed: self.spinner_speed,
            spinner_comet: color("Comet", &pal.comet)?,
            spinner_track: color("Track", &pal.track)?,
            spinner_trail: pal.trail,
            comet_color: color("Sky comet", &pal.comet_color)?,
            sky_glow: pal.sky_glow,
            error_color: color("Error", &pal.error_color)?,
            star_density: self.star_density,
            star_twinkle: self.star_twinkle,
            comet_enabled: self.comet_enabled,
            comet_interval: self.comet_interval,
            cloud_amount: self.cloud_amount,
            cloud_speed: self.cloud_speed,
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
        Message::Tick | Message::SelectTab(_) | Message::ToggleHelp(_) | Message::ToggleExpert(_)
    );
    match message {
        Message::Set(param, value) => state.set(param, value),
        Message::CardAlpha(v) => {
            let pal = if state.editing_day { &mut state.day } else { &mut state.night };
            if let Some(mut col) = Color::parse(pal.card.trim()) {
                col.a = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                pal.card = col.to_hex();
            }
        }
        Message::SpinnerGlow(v) => {
            let pal = if state.editing_day { &mut state.day } else { &mut state.night };
            pal.glow = v.clamp(0.0, 1.0);
        }
        Message::SpinnerSpeed(v) => state.spinner_speed = v.clamp(0.0, 8.0),
        Message::SpinnerTrail(v) => {
            let pal = if state.editing_day { &mut state.day } else { &mut state.night };
            pal.trail = v.clamp(0.15, 1.0);
        }
        Message::EditDay(on) => state.editing_day = on,
        Message::SelectTab(t) => state.tab = t,
        Message::ToggleHelp(on) => state.help_on = on,
        Message::ToggleExpert(on) => state.expert = on,
        Message::ToggleClock(on) => state.show_clock = on,
        Message::ToggleAnimate(on) => state.animate = on,
        // Per-variant sky glow.
        Message::SkyGlow(v) => {
            let pal = if state.editing_day { &mut state.day } else { &mut state.night };
            pal.sky_glow = v.clamp(0.0, 2.0);
        }
        // Shared sky/spinner/card/behavior controls.
        Message::StarDensity(v) => state.star_density = v.clamp(0.0, 1.0),
        Message::StarTwinkle(v) => state.star_twinkle = v.clamp(0.0, 4.0),
        Message::CometEnabled(on) => state.comet_enabled = on,
        Message::CometInterval(v) => state.comet_interval = v.clamp(3.5, 30.0),
        Message::CloudAmount(v) => state.cloud_amount = v.clamp(0.0, 2.0),
        Message::CloudSpeed(v) => state.cloud_speed = v.clamp(0.0, 4.0),
        Message::SpinnerSize(v) => state.spinner_size = v.clamp(24.0, 120.0),
        Message::SpinnerPulse(v) => state.spinner_pulse = v.clamp(0.0, 3.0),
        Message::CardShadowBlur(v) => state.card_shadow_blur = v.clamp(0.0, 80.0),
        Message::CardShadowOpacity(v) => state.card_shadow_opacity = v.clamp(0.0, 1.0),
        Message::AccentBreathing(v) => state.accent_breathing = v.clamp(0.0, 4.0),
        Message::FieldRadius(v) => state.field_radius = v.clamp(0.0, 30.0),
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

    let preview = container(preview_card(theme, state.anim))
        .center_x(Length::Fill)
        .center_y(Length::Fill);

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

    column![
        header,
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

fn colors_tab<'a>(_state: &'a State, pal: &'a Palette, card_a: f32, h: bool) -> Element<'a, Message> {
    let assets = group(
        "ASSETS",
        column![
            helped(plain_row("Wallpaper", &pal.wallpaper, "(animated sky)", Param::Wallpaper), "Full-screen image; blank uses the animated sky.", h),
            helped(plain_row("Logo", &pal.logo, "(comet spinner)", Param::Logo), "Image shown on the card; blank uses the comet spinner.", h),
        ].spacing(9).into(),
    );
    let colors = group(
        "COLORS",
        column![
            row![color_cell("BG", &pal.background, Param::Background), color_cell("Card", &pal.card, Param::Card)].spacing(10),
            row![color_cell("Field", &pal.field, Param::Field), color_cell("Accent", &pal.accent, Param::Accent)].spacing(10),
            row![color_cell("Text", &pal.foreground, Param::Foreground), color_cell("Muted", &pal.muted, Param::Muted)].spacing(10),
            helped(color_cell("Error", &pal.error_color, Param::ErrorColor), "Status-line color when a login fails.", h),
            helped(
                slider_row("Card opacity", card_a, 0.0..=1.0, 0.01, format!("{}%", (card_a * 100.0).round() as u32), Message::CardAlpha),
                "How see-through the login card is.", h),
        ].spacing(9).into(),
    );
    column![assets, colors].spacing(14).into()
}

fn sky_tab<'a>(state: &'a State, pal: &'a Palette, h: bool) -> Element<'a, Message> {
    let main = group(
        "SKY",
        column![
            helped(color_cell("Comet color", &pal.comet_color, Param::SkyComet), "Color of the comet that drifts across the background.", h),
            helped(slider_row("Sky glow", pal.sky_glow, 0.0..=1.5, 0.01, format!("{:.2}", pal.sky_glow), Message::SkyGlow),
                "Night indigo haze / daytime sun-halo strength.", h),
            helped(slider_row("Star density", state.star_density, 0.0..=1.0, 0.01, format!("{}%", (state.star_density * 100.0).round() as u32), Message::StarDensity),
                "How many stars fill the night sky.", h),
            helped(slider_row("Twinkle", state.star_twinkle, 0.0..=4.0, 0.1, format!("{:.1}×", state.star_twinkle), Message::StarTwinkle),
                "How fast the stars sparkle.", h),
            helped(toggle_row("Background comet", state.comet_enabled, Message::CometEnabled),
                "Show the comet that sweeps across the sky.", h),
            helped(slider_row("Comet every", state.comet_interval, 3.5..=30.0, 0.5, format!("{:.0}s", state.comet_interval), Message::CometInterval),
                "Seconds between comet sweeps.", h),
            helped(slider_row("Cloud cover", state.cloud_amount, 0.0..=2.0, 0.05, format!("{:.0}%", state.cloud_amount * 100.0), Message::CloudAmount),
                "Daytime cloud coverage (day theme only).", h),
            helped(slider_row("Cloud drift", state.cloud_speed, 0.0..=4.0, 0.1, format!("{:.1}×", state.cloud_speed), Message::CloudSpeed),
                "How fast daytime clouds move.", h),
        ].spacing(9).into(),
    );
    let advanced = state.expert.then(|| group(
        "SKY · ADVANCED",
        column![
            helped(slider_row("Glow falloff", state.glow_falloff, 0.5..=10.0, 0.1, format!("{:.1}", state.glow_falloff), Message::GlowFalloff),
                "Tightness of the night sky-glow (higher = smaller).", h),
            helped(slider_row("Nebula", state.nebula_amount, 0.0..=0.5, 0.01, format!("{:.2}", state.nebula_amount), Message::NebulaAmount),
                "Amount of cloudy nebula haze at night.", h),
            helped(slider_row("Comet tail", state.comet_tail_decay, 2.0..=30.0, 0.5, format!("{:.1}", state.comet_tail_decay), Message::CometTailDecay),
                "How fast the sky comet's tail fades (higher = shorter).", h),
        ].spacing(9).into(),
    ));
    let mut col = column![main].spacing(14);
    if let Some(adv) = advanced {
        col = col.push(adv);
    }
    col.into()
}

fn spinner_tab<'a>(state: &'a State, pal: &'a Palette, h: bool) -> Element<'a, Message> {
    let main = group(
        "SPINNER",
        column![
            row![color_cell("Comet", &pal.comet, Param::SpinnerComet), color_cell("Track", &pal.track, Param::SpinnerTrack)].spacing(10),
            helped(slider_row("Trail", pal.trail, 0.15..=1.0, 0.01, format!("{}%", (pal.trail * 100.0).round() as u32), Message::SpinnerTrail),
                "Length of the comet's tail.", h),
            helped(slider_row("Glow", pal.glow, 0.0..=1.0, 0.01, format!("{}%", (pal.glow * 100.0).round() as u32), Message::SpinnerGlow),
                "Head bloom (0 = crisp; bands on a light card).", h),
            helped(slider_row("Speed", state.spinner_speed, 0.0..=6.0, 0.1, format!("{:.1}", state.spinner_speed), Message::SpinnerSpeed),
                "Rotation speed.", h),
            helped(slider_row("Size", state.spinner_size, 24.0..=120.0, 1.0, format!("{:.0}px", state.spinner_size), Message::SpinnerSize),
                "Diameter of the card's comet spinner.", h),
            helped(slider_row("Pulse", state.spinner_pulse, 0.0..=3.0, 0.1, format!("{:.1}×", state.spinner_pulse), Message::SpinnerPulse),
                "How fast the spinner head breathes (0 = steady).", h),
        ].spacing(9).into(),
    );
    let advanced = state.expert.then(|| group(
        "SPINNER · ADVANCED",
        column![
            helped(slider_row("Orbit ring", state.spinner_ring, 0.0..=0.5, 0.01, format!("{:.2}", state.spinner_ring), Message::SpinnerRing),
                "Brightness of the spinner's static orbit ring.", h),
        ].spacing(9).into(),
    ));
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
            helped(plain_row("Card rounding", &state.corner_radius, "16", Param::CornerRadius), "Corner radius of the login card (px).", h),
            helped(plain_row("Card width", &state.card_width, "300", Param::CardWidth), "Width of the login card (px).", h),
            helped(slider_row("Field rounding", state.field_radius, 0.0..=30.0, 1.0, format!("{:.0}px", state.field_radius), Message::FieldRadius),
                "Corner radius of inputs and buttons (px).", h),
            helped(slider_row("Shadow blur", state.card_shadow_blur, 0.0..=80.0, 1.0, format!("{:.0}px", state.card_shadow_blur), Message::CardShadowBlur),
                "Softness of the card's drop shadow.", h),
            helped(slider_row("Shadow strength", state.card_shadow_opacity, 0.0..=1.0, 0.01, format!("{}%", (state.card_shadow_opacity * 100.0).round() as u32), Message::CardShadowOpacity),
                "Darkness of the card's drop shadow.", h),
            helped(slider_row("Accent pulse", state.accent_breathing, 0.0..=4.0, 0.1, format!("{:.1}×", state.accent_breathing), Message::AccentBreathing),
                "Speed of the card's glowing accent edge (0 = steady).", h),
        ].spacing(9).into(),
    );
    column![g].spacing(14).into()
}

fn behavior_tab<'a>(state: &'a State, h: bool) -> Element<'a, Message> {
    let g = group(
        "BEHAVIOR",
        column![
            helped(plain_row("Font", &state.font, "(stock font)", Param::Font), "Installed font family; blank = stock.", h),
            helped(toggle_row("Clock + date", state.show_clock, Message::ToggleClock), "Show the time and date on the card.", h),
            helped(toggle_row("24-hour clock", state.clock_24h, Message::Clock24h), "Use 24-hour time instead of AM/PM.", h),
            helped(toggle_row("Animate sky", state.animate, Message::ToggleAnimate), "Run the stars + comet animation.", h),
            helped(slider_row("Launch fade", state.fade_ms, 0.0..=2000.0, 10.0, format!("{:.0}ms", state.fade_ms), Message::FadeMs),
                "Fade-in time when the greeter opens.", h),
            helped(
                row![
                    color_label("Day window"),
                    text_input("07:00", &state.day_start).on_input(|v| Message::Set(Param::DayStart, v)).padding(6).size(14).style(input_style),
                    color_label("to"),
                    text_input("19:00", &state.day_end).on_input(|v| Message::Set(Param::DayEnd, v)).padding(6).size(14).style(input_style),
                ].spacing(8).align_y(Alignment::Center).into(),
                "Local times when the day theme is used.", h),
        ].spacing(9).into(),
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
        background: Some(Background::Color(IColor::from_rgba8(0xff, 0xff, 0xff, 0.022))),
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
        background: Some(Background::Color(IColor::from_rgba8(0x7a, 0xa2, 0xf7, 0.14))),
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

fn color_cell<'a>(label: &'a str, value: &'a str, param: Param) -> Element<'a, Message> {
    row![
        text(label)
            .size(12)
            .width(Length::Fixed(44.0))
            .color(c(LABEL.0, LABEL.1, LABEL.2)),
        text_input("", value)
            .on_input(move |v| Message::Set(param, v))
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
            .width(Length::Fixed(52.0))
            .height(Length::Fixed(52.0))
            .into(),
    };

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

    let body = column![header, logo, field("user", t), field("password", t), sign_in]
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
        background: Some(Background::Color(iced::Color::from_rgba8(0x0e, 0x0f, 0x16, 0.74))),
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
