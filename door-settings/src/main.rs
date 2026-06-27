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
    button, canvas, column, container, image, row, scrollable, slider, text, text_input, toggler,
    Space,
};
use iced::{
    Alignment, Background, Border, Color as IColor, ContentFit, Element, Length, Shadow,
    Subscription, Task, Vector,
};

use door_theme::sky::{self, Sky};
use door_theme::{Color, Theme};

fn main() -> iced::Result {
    iced::application(State::new, update, view)
        .title("door — greeter settings")
        .style(app_style)
        .subscription(subscription)
        .run()
}

fn app_style(state: &State, _theme: &iced::Theme) -> iced::theme::Style {
    // Use the previewed variant's background so the day preview sits on a light bg
    // (not a hardcoded dark one) — the sky + card render over it accurately.
    let t = state.preview_theme();
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
    Font,
    CornerRadius,
    CardWidth,
    DayStart,
    DayEnd,
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
    // Which variant is being edited / previewed.
    editing_day: bool,
    status: String,
    anim: f32,
    started: Instant,
    stars: Vec<sky::Star>,
}

fn minutes_to_hhmm(m: u32) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

impl State {
    fn new() -> Self {
        let (night, day, (start, end)) = Theme::load_pair();
        State {
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
            // Dev: start on the day variant when DOOR_SETTINGS_DAY is set.
            editing_day: std::env::var_os("DOOR_SETTINGS_DAY").is_some(),
            status: "Loaded night + day themes.".to_string(),
            anim: 0.0,
            started: Instant::now(),
            stars: sky::stars(),
        }
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
        })
    }

    /// The theme to render the preview with — the active variant, or the default if
    /// a field doesn't parse yet (the status line shows the error).
    fn preview_theme(&self) -> Theme {
        self.build(self.editing_day).unwrap_or_else(|_| {
            if self.editing_day {
                Theme::day()
            } else {
                Theme::default()
            }
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
        Message::ToggleClock(on) => state.show_clock = on,
        Message::ToggleAnimate(on) => state.animate = on,
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
    let theme = state.preview_theme();

    let panel = container(scrollable(controls(state)))
        .width(Length::Fixed(372.0))
        .height(Length::Fill)
        .padding(16)
        .style(glass_panel);
    let left = container(panel).padding(16);

    let preview = container(preview_card(&theme, state.anim))
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    let content = row![left, preview].height(Length::Fill);

    let sky_layer: Element<Message> = if theme.animate {
        canvas(Sky {
            stars: state.stars.clone(),
            anim: state.anim,
            fade: 1.0,
            day: theme.is_day,
        })
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
    let card_a = Color::parse(pal.card.trim())
        .map(|col| col.a as f32 / 255.0)
        .unwrap_or(1.0);
    column![
        text("Greeter").size(26).color(c(FG.0, FG.1, FG.2)),
        text("Edits preview live · Save asks for your password")
            .size(12)
            .color(c(MUTED.0, MUTED.1, MUTED.2)),
        // Night / Day editor toggle.
        toggler(state.editing_day)
            .label(if state.editing_day { "Editing: Day" } else { "Editing: Night" })
            .on_toggle(Message::EditDay)
            .size(18)
            .text_size(14),
        section("WALLPAPER & ASSETS"),
        plain_row("Wallpaper", &pal.wallpaper, "(animated sky)", Param::Wallpaper),
        plain_row("Logo", &pal.logo, "(comet spinner)", Param::Logo),
        section("COLORS"),
        row![
            color_cell("BG", &pal.background, Param::Background),
            color_cell("Card", &pal.card, Param::Card),
        ]
        .spacing(10),
        row![
            color_cell("Field", &pal.field, Param::Field),
            color_cell("Accent", &pal.accent, Param::Accent),
        ]
        .spacing(10),
        row![
            color_cell("Text", &pal.foreground, Param::Foreground),
            color_cell("Muted", &pal.muted, Param::Muted),
        ]
        .spacing(10),
        row![
            text("Card opacity")
                .size(13)
                .width(Length::Fixed(92.0))
                .color(c(LABEL.0, LABEL.1, LABEL.2)),
            slider(0.0..=1.0, card_a, Message::CardAlpha).step(0.01),
            text(format!("{}%", (card_a * 100.0).round() as u32))
                .size(12)
                .width(Length::Fixed(38.0))
                .color(c(MUTED.0, MUTED.1, MUTED.2)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        section("SPINNER"),
        row![
            color_cell("Comet", &pal.comet, Param::SpinnerComet),
            color_cell("Track", &pal.track, Param::SpinnerTrack),
        ]
        .spacing(10),
        row![
            text("Trail")
                .size(13)
                .width(Length::Fixed(92.0))
                .color(c(LABEL.0, LABEL.1, LABEL.2)),
            slider(0.15..=1.0, pal.trail, Message::SpinnerTrail).step(0.01),
            text(format!("{}%", (pal.trail * 100.0).round() as u32))
                .size(12)
                .width(Length::Fixed(38.0))
                .color(c(MUTED.0, MUTED.1, MUTED.2)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Glow")
                .size(13)
                .width(Length::Fixed(92.0))
                .color(c(LABEL.0, LABEL.1, LABEL.2)),
            slider(0.0..=1.0, pal.glow, Message::SpinnerGlow).step(0.01),
            text(format!("{}%", (pal.glow * 100.0).round() as u32))
                .size(12)
                .width(Length::Fixed(38.0))
                .color(c(MUTED.0, MUTED.1, MUTED.2)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Speed")
                .size(13)
                .width(Length::Fixed(92.0))
                .color(c(LABEL.0, LABEL.1, LABEL.2)),
            slider(0.0..=6.0, state.spinner_speed, Message::SpinnerSpeed).step(0.1),
            text(format!("{:.1}", state.spinner_speed))
                .size(12)
                .width(Length::Fixed(38.0))
                .color(c(MUTED.0, MUTED.1, MUTED.2)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        section("SHARED (font, layout, behavior)"),
        plain_row("Font", &state.font, "(stock font)", Param::Font),
        plain_row("Corner radius", &state.corner_radius, "", Param::CornerRadius),
        plain_row("Card width", &state.card_width, "", Param::CardWidth),
        row![
            color_label("Day from"),
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
        .align_y(Alignment::Center),
        toggler(state.show_clock)
            .label("Clock + date")
            .on_toggle(Message::ToggleClock)
            .size(18)
            .text_size(14),
        toggler(state.animate)
            .label("Animate sky (stars + comet)")
            .on_toggle(Message::ToggleAnimate)
            .size(18)
            .text_size(14),
        row![
            primary_button("Save", Message::Save),
            ghost_button("Open in greeter", Message::OpenInGreeter),
            ghost_button("Reset", Message::Reset),
        ]
        .spacing(8),
        text(state.status.clone())
            .size(12)
            .color(c(MUTED.0, MUTED.1, MUTED.2)),
    ]
    .spacing(7)
    .into()
}

fn section(title: &str) -> Element<'static, Message> {
    text(title.to_string())
        .size(11)
        .color(c(MUTED.0, MUTED.1, MUTED.2))
        .into()
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
        None => canvas(sky::Spinner {
            anim,
            fade: 1.0,
            comet: t.spinner_comet.iced(),
            track: t.spinner_track.iced(),
            trail: t.spinner_trail,
            glow: t.spinner_glow,
            speed: t.spinner_speed,
        })
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
