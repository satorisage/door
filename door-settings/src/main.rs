//! door-settings — a standalone editor for the greeter theme.
//!
//! It is *not* the greeter and holds no authority: it loads the resolved theme
//! ([`door_theme::Theme::load`]), lets you edit it, **previews** by launching the
//! real `door-greeter` in dev mode against a draft config (a true preview, no UI
//! duplication), and **saves** to `/etc/door/greeter.toml` via `pkexec` (the file
//! is root-owned because the greeter is pre-login). Sharing `door-theme` with the
//! greeter keeps one source of truth for the schema.

use std::path::PathBuf;

use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task};

use door_theme::{Color, Theme};

fn main() -> iced::Result {
    iced::application(State::new, update, view)
        .title("door — greeter settings")
        .run()
}

/// Which text field changed (every editable theme value except the clock toggle).
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
    Preview,
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
    status: String,
}

impl State {
    fn new() -> Self {
        let mut s = State::from_theme(&Theme::load());
        s.status = "Loaded the current theme.".to_string();
        s
    }

    /// Populate the editable fields from a resolved theme.
    fn from_theme(t: &Theme) -> Self {
        let path = |p: &Option<PathBuf>| {
            p.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
        };
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
            status: String::new(),
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

    /// Build a [`Theme`] from the edited fields, or an error describing the first
    /// invalid input (so Preview/Save fail loudly rather than writing garbage).
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
        })
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
        Message::Reset => {
            *state = State::from_theme(&Theme::default());
            state.status = "Reset to the built-in default (not saved).".to_string();
        }
        Message::Preview => match write_draft(state) {
            Ok(path) => {
                // Launch the real greeter in dev mode against the draft — a true
                // preview. `DOOR_GREETER_BIN` overrides the binary for dev runs.
                let bin = std::env::var("DOOR_GREETER_BIN").unwrap_or_else(|_| "door-greeter".into());
                match std::process::Command::new(&bin)
                    .env("DOORD_GREETER_DEV", "1")
                    .env("DOORD_GREETER_CONFIG", &path)
                    .spawn()
                {
                    Ok(_) => state.status = "Preview launched (close its window to return).".into(),
                    Err(e) => state.status = format!("Could not launch '{bin}': {e}"),
                }
            }
            Err(e) => state.status = e,
        },
        Message::Save => match write_draft(state) {
            Ok(path) => {
                // /etc/door is root-owned (the greeter is pre-login), so escalate
                // the install via pkexec — the user gets a polkit prompt.
                match std::process::Command::new("pkexec")
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
                }
            }
            Err(e) => state.status = e,
        },
    }
    Task::none()
}

fn view(state: &State) -> Element<'_, Message> {
    let field = |label: &'static str, value: &str, param: Param| -> Element<Message> {
        row![
            text(label).width(Length::Fixed(130.0)),
            text_input("", value)
                .on_input(move |v| Message::Set(param, v))
                .padding(8),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    };

    let form = column![
        text("Greeter theme").size(26),
        text("Edit, Preview to see it live, Save to apply (asks for your password).")
            .size(13),
        field("Wallpaper", &state.wallpaper, Param::Wallpaper),
        field("Background", &state.background, Param::Background),
        field("Card", &state.card, Param::Card),
        field("Field", &state.field, Param::Field),
        field("Accent", &state.accent, Param::Accent),
        field("Foreground", &state.foreground, Param::Foreground),
        field("Muted", &state.muted, Param::Muted),
        field("Font", &state.font, Param::Font),
        field("Logo", &state.logo, Param::Logo),
        field("Corner radius", &state.corner_radius, Param::CornerRadius),
        field("Card width", &state.card_width, Param::CardWidth),
        checkbox(state.show_clock)
            .label("Show clock + date")
            .on_toggle(Message::ToggleClock),
        row![
            button(text("Preview")).on_press(Message::Preview),
            button(text("Save")).on_press(Message::Save),
            button(text("Reset to default")).on_press(Message::Reset),
        ]
        .spacing(10),
        text(state.status.clone()).size(13),
    ]
    .spacing(12)
    .padding(20)
    .max_width(560);

    container(scrollable(form)).center_x(Length::Fill).into()
}
