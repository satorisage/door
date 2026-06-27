//! door-settings — a standalone editor for the greeter theme.
//!
//! Not the greeter and holds no authority: it loads the resolved theme
//! ([`door_theme::Theme::load`]), edits it with a **live in-window preview** (the
//! real wallpaper + the shared animated sky + a mock card, rendered from the
//! current draft), and saves to `/etc/door/greeter.toml` via `pkexec` (the file is
//! root-owned because the greeter is pre-login). Sharing `door-theme` with the
//! greeter keeps one source of truth for both the schema and the look.

use std::path::PathBuf;
use std::time::Duration;

use futures::SinkExt;
use iced::widget::{
    button, canvas, checkbox, column, container, image, row, scrollable, text, text_input, Space,
};
use iced::{
    Alignment, Background, Border, ContentFit, Element, Length, Shadow, Subscription, Task, Vector,
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

fn app_style(_state: &State, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: iced::Color::from_rgb8(0x16, 0x16, 0x1e),
        text_color: iced::Color::from_rgb8(0xc0, 0xca, 0xf5),
    }
}

/// Which text field changed.
#[derive(Debug, Clone, Copy)]
enum Param {
    Wallpaper,
    Background,
    Card,
    Field,
    Accent,
    Foreground,
    Muted,
    Font,
    Logo,
    CornerRadius,
    CardWidth,
}

#[derive(Debug, Clone)]
enum Message {
    Set(Param, String),
    ToggleClock(bool),
    ToggleAnimate(bool),
    Tick,
    OpenInGreeter,
    Save,
    Reset,
}

struct State {
    wallpaper: String,
    background: String,
    card: String,
    field: String,
    accent: String,
    foreground: String,
    muted: String,
    font: String,
    logo: String,
    corner_radius: String,
    card_width: String,
    show_clock: bool,
    animate: bool,
    status: String,
    // Live-preview animation.
    anim: f32,
    stars: Vec<sky::Star>,
}

impl State {
    fn new() -> Self {
        let mut s = State::from_theme(&Theme::load());
        s.status = "Loaded the current theme.".to_string();
        s
    }

    fn from_theme(t: &Theme) -> Self {
        let path =
            |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        State {
            wallpaper: path(&t.wallpaper),
            background: t.background.to_hex(),
            card: t.card.to_hex(),
            field: t.field.to_hex(),
            accent: t.accent.to_hex(),
            foreground: t.foreground.to_hex(),
            muted: t.muted.to_hex(),
            font: t.font.clone().unwrap_or_default(),
            logo: path(&t.logo),
            corner_radius: t.corner_radius.to_string(),
            card_width: t.card_width.to_string(),
            show_clock: t.show_clock,
            animate: t.animate,
            status: String::new(),
            anim: 0.0,
            stars: sky::stars(),
        }
    }

    fn set(&mut self, param: Param, value: String) {
        match param {
            Param::Wallpaper => self.wallpaper = value,
            Param::Background => self.background = value,
            Param::Card => self.card = value,
            Param::Field => self.field = value,
            Param::Accent => self.accent = value,
            Param::Foreground => self.foreground = value,
            Param::Muted => self.muted = value,
            Param::Font => self.font = value,
            Param::Logo => self.logo = value,
            Param::CornerRadius => self.corner_radius = value,
            Param::CardWidth => self.card_width = value,
        }
    }

    /// Build a [`Theme`] from the edited fields, or the first invalid-input error.
    fn build(&self) -> Result<Theme, String> {
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
            wallpaper: opt_path(&self.wallpaper),
            background: color("Background", &self.background)?,
            card: color("Card", &self.card)?,
            field: color("Field", &self.field)?,
            accent: color("Accent", &self.accent)?,
            foreground: color("Foreground", &self.foreground)?,
            muted: color("Muted", &self.muted)?,
            logo: opt_path(&self.logo),
            font: {
                let t = self.font.trim();
                (!t.is_empty()).then(|| t.to_string())
            },
            corner_radius: num("Corner radius", &self.corner_radius)?,
            card_width: num("Card width", &self.card_width)?,
            show_clock: self.show_clock,
            animate: self.animate,
        })
    }

    /// The theme to render the preview with — the draft if valid, else the default
    /// (the status line still shows the parse error so nothing is silently wrong).
    fn preview_theme(&self) -> Theme {
        self.build().unwrap_or_default()
    }
}

/// Write the current draft to a temp `greeter.toml`, returning its path.
fn write_draft(state: &State) -> Result<PathBuf, String> {
    let theme = state.build()?;
    let path = std::env::temp_dir().join("door-settings-draft.toml");
    std::fs::write(&path, theme.to_config_string()).map_err(|e| format!("writing draft: {e}"))?;
    Ok(path)
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Set(param, value) => state.set(param, value),
        Message::ToggleClock(on) => state.show_clock = on,
        Message::ToggleAnimate(on) => state.animate = on,
        Message::Tick => state.anim = (state.anim + 0.033) % 10_000.0,
        Message::Reset => {
            *state = State::from_theme(&Theme::default());
            state.status = "Reset to the built-in default (not saved).".to_string();
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
                    state.status = format!("Saved to {} ✓", door_theme::ETC_CONFIG)
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
        Subscription::run(ticker)
    } else {
        Subscription::none()
    }
}

/// ~30 fps tick for the live preview's animated sky.
fn ticker() -> impl futures::Stream<Item = Message> {
    iced_futures::stream::channel(4, |output: futures::channel::mpsc::Sender<Message>| async move {
        std::thread::spawn(move || {
            let mut output = output;
            loop {
                std::thread::sleep(Duration::from_millis(33));
                if futures::executor::block_on(output.send(Message::Tick)).is_err() {
                    break;
                }
            }
        });
        std::future::pending::<()>().await;
    })
}

// ---- view -----------------------------------------------------------------

fn view(state: &State) -> Element<'_, Message> {
    let theme = state.preview_theme();

    // Left: a glass control panel. Right: the live greeter preview, centered.
    let controls = container(scrollable(controls(state)))
        .width(Length::Fixed(380.0))
        .height(Length::Fill)
        .padding(22)
        .style(glass_panel);

    let preview = container(preview_card(&theme))
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    let content = row![controls, preview].height(Length::Fill);

    // The whole window is a live preview: wallpaper + animated sky behind the
    // control panel and the card — "what you're editing, live".
    let sky_layer: Element<Message> = if theme.animate {
        canvas(Sky {
            stars: state.stars.clone(),
            anim: state.anim,
            fade: 1.0,
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

/// The editable control list inside the glass panel.
fn controls(state: &State) -> Element<'_, Message> {
    let row_field = |label: &'static str, value: &str, param: Param| -> Element<Message> {
        row![
            text(label).size(13).width(Length::Fixed(118.0)),
            text_input("", value)
                .on_input(move |v| Message::Set(param, v))
                .padding(7)
                .size(14),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };

    column![
        text("Greeter theme").size(24),
        text("Edits preview live. Save asks for your password.").size(12),
        row_field("Wallpaper", &state.wallpaper, Param::Wallpaper),
        row_field("Background", &state.background, Param::Background),
        row_field("Card", &state.card, Param::Card),
        row_field("Field", &state.field, Param::Field),
        row_field("Accent", &state.accent, Param::Accent),
        row_field("Foreground", &state.foreground, Param::Foreground),
        row_field("Muted", &state.muted, Param::Muted),
        row_field("Font", &state.font, Param::Font),
        row_field("Logo", &state.logo, Param::Logo),
        row_field("Corner radius", &state.corner_radius, Param::CornerRadius),
        row_field("Card width", &state.card_width, Param::CardWidth),
        checkbox(state.show_clock)
            .label("Show clock + date")
            .on_toggle(Message::ToggleClock)
            .size(16),
        checkbox(state.animate)
            .label("Animate sky (stars + comet)")
            .on_toggle(Message::ToggleAnimate)
            .size(16),
        row![
            button(text("Save").size(14)).on_press(Message::Save),
            button(text("Open in greeter").size(14)).on_press(Message::OpenInGreeter),
            button(text("Reset").size(14)).on_press(Message::Reset),
        ]
        .spacing(8),
        text(state.status.clone()).size(12),
    ]
    .spacing(12)
    .into()
}

/// A non-interactive mock of the greeter card, themed from the draft.
fn preview_card(t: &Theme) -> Element<'static, Message> {
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
        field("user", t),
        field("password", t),
        sign_in,
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

/// The frosted control panel: a translucent dark glass with a soft edge + shadow.
fn glass_panel(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(iced::Color::from_rgba8(0x0e, 0x0f, 0x16, 0.78))),
        border: Border {
            radius: 0.0.into(),
            width: 1.0,
            color: iced::Color::from_rgba8(0x7a, 0xa2, 0xf7, 0.18),
        },
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: Vector::new(6.0, 0.0),
            blur_radius: 30.0,
        },
        ..Default::default()
    }
}
