//! The greeter UI: an Iced `wlr-layer-shell` overlay wired to the daemon.
//!
//! The app itself holds no authority. A background worker owns the protocol
//! [`Client`](crate::client::Client) (blocking socket I/O) and talks to this UI
//! over two channels: the UI sends [`Command`]s to the worker, and the worker
//! emits [`Message`]s back through an Iced subscription. The worker hands the UI
//! its command channel via [`Message::WorkerReady`] (the standard Iced
//! external-worker handshake), so nothing global is needed.
//!
//! Credentials never leave this process except as the daemon's PAM reply: the
//! typed password lives in the UI state only until the worker is told to send it,
//! then the field is cleared.

use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use futures::SinkExt;
use iced::widget::{
    button, canvas, column, container, image, pick_list, row, stack, text, text_input, Space,
};
use iced::{
    keyboard, window, Alignment, Background, Border, ContentFit, Element, Font, Length, Shadow,
    Subscription, Task, Vector,
};

use protocol::{PowerAction, Secret, Session};

use crate::client::{AuthStep, Client, StartOutcome, DEFAULT_SOCKET};
use door_theme::sky::{self, Sky};
use door_theme::{Color, Theme};

/// The resolved theme, loaded once. `run` needs it for the default font before the
/// app state exists, and the UI reads it every frame — so it lives here, not in
/// `State`.
fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(Theme::load)
}

/// Run the greeter (D-0007): a plain `iced` fullscreen toplevel, hosted by `cage`
/// on the greeter VT. As the sole client on its own compositor it needs nothing
/// `wlr-layer-shell` offers (v1 has no lock screen), and `cage` — the smallest
/// pre-auth surface — does not advertise it.
///
/// Production: fullscreen. Dev (`DOORD_GREETER_DEV` set): a normal window, so the
/// greeter can be run nested in any session for testing — no keyboard grab, just a
/// window (Ctrl-C the launching terminal, or close it, to quit).
pub fn run() -> iced::Result {
    let fullscreen = std::env::var_os("DOORD_GREETER_DEV").is_none();

    let mut app = iced::application(State::new, update, view)
        .title("door")
        .window(window::Settings {
            fullscreen,
            ..Default::default()
        })
        .style(app_style)
        .subscription(subscription);

    // Render in the themed font if one is configured and installed system-wide.
    // iced wants a 'static family name, so leak the single, process-lifetime string.
    if let Some(name) = theme().font.clone() {
        let name: &'static str = Box::leak(name.into_boxed_str());
        app = app.default_font(Font::with_name(name));
    }
    app.run()
}

/// Window-level appearance from the theme: the solid background (also shown at any
/// wallpaper letterbox edge) and the default text color.
fn app_style(state: &State, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: state.theme.background.iced(),
        text_color: state.theme.foreground.iced(),
    }
}

/// The greeter's subscriptions: the daemon worker event stream, a 1 Hz clock, and
/// the one-shot launch fade.
fn subscription(state: &State) -> Subscription<Message> {
    let mut subs = vec![
        Subscription::run(daemon_worker),
        Subscription::run(clock_ticker),
        Subscription::run(fade_ticker),
        // Tab / Shift-Tab cycle focus through the fields and the sign-in button.
        iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Tab),
                modifiers,
                ..
            }) => Some(if modifiers.shift() {
                Message::FocusPrev
            } else {
                Message::FocusNext
            }),
            _ => None,
        }),
    ];
    // Drive the animation off the compositor's frame clock (vsync — 60/120/144 Hz),
    // not a fixed-rate thread, so motion is buttery smooth. Only when animated.
    if state.theme.animate {
        subs.push(iced::window::frames().map(|_| Message::AnimTick));
    }
    Subscription::batch(subs)
}

/// Emit a [`Message::Tick`] once a second so the card clock stays current. A small
/// thread sleeps and pushes through the Iced channel (same shape as the daemon
/// worker) — avoids pulling an async timer/runtime feature onto the greeter.
fn clock_ticker() -> impl futures::Stream<Item = Message> {
    iced_futures::stream::channel(4, |output: futures::channel::mpsc::Sender<Message>| async move {
        std::thread::spawn(move || {
            let mut output = output;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if futures::executor::block_on(output.send(Message::Tick)).is_err() {
                    break;
                }
            }
        });
        std::future::pending::<()>().await;
    })
}

/// Drive the launch fade-in: ~24 [`Message::Fade`] ticks over ~380 ms, then the
/// stream ends (bounded — no perpetual repaint on the idle greeter).
fn fade_ticker() -> impl futures::Stream<Item = Message> {
    iced_futures::stream::channel(4, |output: futures::channel::mpsc::Sender<Message>| async move {
        std::thread::spawn(move || {
            let mut output = output;
            for _ in 0..24 {
                std::thread::sleep(Duration::from_millis(16));
                if futures::executor::block_on(output.send(Message::Fade)).is_err() {
                    return;
                }
            }
        });
        std::future::pending::<()>().await;
    })
}

/// A startable session, rendered by name in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChoice {
    id: String,
    name: String,
}

impl std::fmt::Display for SessionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// Where the greeter is in the login flow.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// Connecting / waiting for the worker and the session list.
    Connecting,
    /// Idle, ready for the user to enter credentials.
    Ready,
    /// Authentication is in flight (waiting on PAM prompts / verdict).
    Authenticating,
    /// The session was started; the greeter is stepping aside.
    Started,
}

/// What the UI asks the worker to do.
#[derive(Debug)]
pub enum Command {
    Authenticate { username: String },
    Reply(Secret),
    Start { session_id: String },
    Power(PowerAction),
}

#[derive(Debug, Clone)]
pub enum Message {
    // From the worker:
    WorkerReady(mpsc::Sender<Command>),
    Sessions(Vec<Session>),
    /// PAM wants input; `secret` ⇒ it is the password (auto-answered if typed).
    Prompt { text: String, secret: bool },
    Notice(String),
    AuthSucceeded,
    AuthFailed(String),
    SessionStarted,
    DaemonError(String),
    Fatal(String),
    /// 1 Hz clock tick — refreshes the card clock + date.
    Tick,
    /// One step of the launch fade-in.
    Fade,
    /// Continuous ambient animation tick (twinkle + breathing card edge).
    AnimTick,
    /// Tab / Shift-Tab focus traversal across the fields and sign-in.
    FocusNext,
    FocusPrev,
    // From the UI:
    UsernameChanged(String),
    PasswordChanged(String),
    SessionPicked(SessionChoice),
    LoginPressed,
    PowerPressed(PowerAction),
}

struct State {
    phase: Phase,
    sessions: Vec<SessionChoice>,
    selected: Option<SessionChoice>,
    username: String,
    password: String,
    status: String,
    cmd_tx: Option<mpsc::Sender<Command>>,
    /// The resolved look (a clone of the process-wide [`theme`]).
    theme: Theme,
    /// Current local time, `HH:MM`, refreshed by [`Message::Tick`].
    clock: String,
    /// Current local date, e.g. `Friday, June 27`.
    date: String,
    /// Launch fade-in progress, 0.0 → 1.0 (driven by [`Message::Fade`]).
    fade: f32,
    /// Animation clock in seconds — recomputed from `started` every frame so motion
    /// is time-accurate and smooth (not a fixed per-tick increment).
    anim: f32,
    /// When the greeter started, the zero point for `anim`.
    started: Instant,
    /// Fixed ambient starfield positions (generated once so they don't jump).
    stars: Vec<sky::Star>,
}

impl State {
    fn new() -> Self {
        State {
            phase: Phase::Connecting,
            sessions: Vec::new(),
            selected: None,
            username: String::new(),
            password: String::new(),
            status: "Connecting to doord…".to_string(),
            cmd_tx: None,
            // Pick the day or night variant by the local clock — the greeter runs
            // pre-login, so it can't read the user's color scheme; time is the trigger.
            theme: Theme::load_at(now_minutes()),
            clock: now_hm(),
            date: now_date(),
            fade: 0.0,
            anim: 0.0,
            started: Instant::now(),
            stars: sky::stars(),
        }
    }

    /// Send a command to the worker, noting if the worker is gone.
    fn send(&mut self, command: Command) {
        match &self.cmd_tx {
            Some(tx) => {
                if tx.send(command).is_err() {
                    self.status = "Lost the connection to doord.".to_string();
                    self.phase = Phase::Connecting;
                }
            }
            None => self.status = "Not connected to doord yet.".to_string(),
        }
    }
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    let mut task = Task::none();
    match message {
        Message::WorkerReady(tx) => {
            state.cmd_tx = Some(tx);
            state.status = "Select a session and sign in.".to_string();
            if state.phase == Phase::Connecting {
                state.phase = Phase::Ready;
            }
        }
        Message::Sessions(sessions) => {
            state.sessions = sessions
                .into_iter()
                .map(|s| SessionChoice {
                    id: s.id,
                    name: s.name,
                })
                .collect();
            if state.selected.is_none() {
                state.selected = state.sessions.first().cloned();
            }
        }
        Message::Prompt { text, secret } => {
            // Show what PAM is asking. The daemon's only prompt is the password:
            // if the user already typed it, answer immediately; otherwise the
            // password field is there for them to type and submit.
            state.status = text;
            if secret && !state.password.is_empty() {
                let reply = Secret::new(std::mem::take(&mut state.password));
                state.send(Command::Reply(reply));
            }
        }
        Message::Notice(text) => state.status = text,
        Message::AuthSucceeded => {
            state.password.clear();
            match &state.selected {
                Some(choice) => {
                    state.status = format!("Starting {}…", choice.name);
                    let id = choice.id.clone();
                    state.send(Command::Start { session_id: id });
                }
                None => {
                    state.status = "Authenticated, but no session selected.".to_string();
                    state.phase = Phase::Ready;
                }
            }
        }
        Message::AuthFailed(reason) => {
            state.password.clear();
            state.status = reason;
            state.phase = Phase::Ready;
        }
        Message::SessionStarted => {
            state.phase = Phase::Started;
            state.status = "Session started.".to_string();
            // The greeter's job is done: step aside so the host compositor (cage)
            // exits and frees the VT for the session the daemon just launched.
            task = iced::exit();
        }
        Message::DaemonError(message) => {
            state.password.clear();
            state.status = message;
            state.phase = Phase::Ready;
        }
        Message::Fatal(message) => {
            state.status = format!("Cannot reach doord: {message}");
            state.phase = Phase::Connecting;
        }
        Message::UsernameChanged(value) => state.username = value,
        Message::PasswordChanged(value) => state.password = value,
        Message::SessionPicked(choice) => state.selected = Some(choice),
        Message::LoginPressed => {
            if state.username.is_empty() {
                state.status = "Enter a username.".to_string();
            } else if state.selected.is_none() {
                state.status = "Select a session.".to_string();
            } else {
                state.phase = Phase::Authenticating;
                state.status = "Authenticating…".to_string();
                let username = state.username.clone();
                state.send(Command::Authenticate { username });
            }
        }
        Message::PowerPressed(action) => state.send(Command::Power(action)),
        Message::Tick => {
            state.clock = now_hm();
            state.date = now_date();
        }
        Message::Fade => state.fade = (state.fade + 1.0 / 24.0).min(1.0),
        // Recompute the clock from real elapsed time each frame — smooth, jitter-free.
        Message::AnimTick => state.anim = state.started.elapsed().as_secs_f32() % 10_000.0,
        Message::FocusNext => task = iced::widget::operation::focus_next(),
        Message::FocusPrev => task = iced::widget::operation::focus_previous(),
    }
    task
}

/// Local `tm` for the current epoch second, or `None` if the conversion fails.
/// `localtime_r` fills a caller-owned struct — no date/time crate on the pre-auth
/// surface.
fn local_tm() -> Option<libc::tm> {
    // SAFETY: `time(NULL)` returns epoch seconds; `localtime_r` fills our owned `tm`
    // and returns null on failure. No shared state, no allocation.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            None
        } else {
            Some(tm)
        }
    }
}

/// Local wall-clock time as `HH:MM`.
fn now_hm() -> String {
    match local_tm() {
        Some(tm) => format!("{:02}:{:02}", tm.tm_hour, tm.tm_min),
        None => String::new(),
    }
}

/// Local time as minutes since midnight (for the day/night window). Noon on failure.
fn now_minutes() -> u32 {
    match local_tm() {
        Some(tm) => (tm.tm_hour.clamp(0, 23) as u32) * 60 + (tm.tm_min.clamp(0, 59) as u32),
        None => 12 * 60,
    }
}

/// Local date as `Weekday, Month D` (e.g. `Friday, June 27`).
fn now_date() -> String {
    const DAYS: [&str; 7] = [
        "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    match local_tm() {
        Some(tm) => {
            let day = DAYS.get(tm.tm_wday as usize).copied().unwrap_or("");
            let month = MONTHS.get(tm.tm_mon as usize).copied().unwrap_or("");
            format!("{day}, {month} {}", tm.tm_mday)
        }
        None => String::new(),
    }
}

fn view(state: &State) -> Element<'_, Message> {
    let t = &state.theme;
    let f = state.fade.clamp(0.0, 1.0);
    let fg = t.foreground.iced_alpha(f);
    let muted = t.muted.iced_alpha(f);

    // Clock + date — the minimal focal point at the top of the card.
    let header: Element<Message> = if t.show_clock {
        column![
            text(state.clock.clone()).size(56).color(fg),
            text(state.date.clone()).size(13).color(muted),
        ]
        .spacing(2)
        .align_x(Alignment::Center)
        .into()
    } else {
        Space::new().into()
    };

    // Logo: a user-set image overrides; otherwise the native animated comet
    // spinner (the boot throbber's sibling) is the default card emblem.
    let logo: Element<Message> = match &t.logo {
        Some(path) => image(image::Handle::from_path(path))
            .height(Length::Fixed(56.0))
            .into(),
        None => canvas(sky::Spinner {
            anim: state.anim,
            fade: f,
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

    let username = text_input("user", &state.username)
        .on_input(Message::UsernameChanged)
        .on_submit(Message::LoginPressed)
        .padding(11)
        .size(15)
        .style(field_style(t, f));

    let password = text_input("password", &state.password)
        .on_input(Message::PasswordChanged)
        .on_submit(Message::LoginPressed)
        .secure(true)
        .padding(11)
        .size(15)
        .style(field_style(t, f));

    let busy = matches!(state.phase, Phase::Authenticating | Phase::Started);
    let mut login = button(text("Sign in").width(Length::Fill).center().size(15))
        .width(Length::Fill)
        .padding(11)
        .style(button_style(t, f));
    if !busy {
        login = login.on_press(Message::LoginPressed);
    }

    // Session: a slim, subtle selector under the button.
    let picker = pick_list(
        state.sessions.clone(),
        state.selected.clone(),
        Message::SessionPicked,
    )
    .placeholder("session")
    .text_size(13)
    .padding(8)
    .width(Length::Fill)
    .style(picker_style(t, f));

    // Status only takes space when there is something to say.
    let status: Element<Message> = if state.status.is_empty() {
        Space::new().into()
    } else {
        text(state.status.clone()).size(12).color(muted).into()
    };

    let form = column![header, logo, username, password, login, picker, status]
        .spacing(12)
        .align_x(Alignment::Center);

    let card = container(form)
        .padding(26)
        .width(Length::Fixed(t.card_width))
        .style(card_style(t, f, state.anim));

    let centered = container(card).center_x(Length::Fill).center_y(Length::Fill);

    // Power controls: subtle, outside the card, bottom-right of the screen.
    let power = container(
        row![
            power_button("Suspend", PowerAction::Suspend, t, f),
            power_button("Restart", PowerAction::Reboot, t, f),
            power_button("Shut down", PowerAction::PowerOff, t, f),
        ]
        .spacing(4),
    )
    .align_right(Length::Fill)
    .align_bottom(Length::Fill)
    .padding(18);

    let overlay = stack![centered, power];

    // The animated sky (twinkling starfield + drifting comet) over the wallpaper —
    // only when animation is enabled; otherwise the still wallpaper shows through.
    let scene: Element<Message> = if t.animate {
        let sky = canvas(Sky {
            stars: state.stars.clone(),
            anim: state.anim,
            fade: f,
            day: t.is_day,
        })
        .width(Length::Fill)
        .height(Length::Fill);
        stack![sky, overlay].into()
    } else {
        overlay.into()
    };

    // Wallpaper behind everything, if configured (a missing file just leaves the
    // solid window background from `app_style`).
    match &t.wallpaper {
        Some(path) => {
            let background = image(image::Handle::from_path(path))
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Cover);
            stack![background, scene].into()
        }
        None => scene,
    }
}

/// Lighten a color toward white by `amt` (0.0–1.0) — for button hover.
fn lighten(c: Color, amt: f32) -> Color {
    let mix = |v: u8| (v as f32 + (255.0 - v as f32) * amt).round() as u8;
    Color {
        r: mix(c.r),
        g: mix(c.g),
        b: mix(c.b),
        a: c.a,
    }
}

/// Slim rounded input: a lifted field fill with an accent border on focus.
fn field_style(t: &Theme, fade: f32) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    let field = t.field.iced_alpha(fade);
    let fg = t.foreground.iced_alpha(fade);
    let muted = t.muted.iced_alpha(fade);
    let accent = t.accent.iced_alpha(fade);
    move |_theme, status| {
        let focused = matches!(status, text_input::Status::Focused { .. });
        text_input::Style {
            background: Background::Color(field),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: if focused {
                    accent
                } else {
                    iced::Color::TRANSPARENT
                },
            },
            icon: muted,
            placeholder: muted,
            value: fg,
            selection: accent,
        }
    }
}

/// The accent sign-in button, brighter on hover, with dark text.
fn button_style(t: &Theme, fade: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let accent = t.accent;
    let on_accent = Color {
        r: 0x16,
        g: 0x16,
        b: 0x1e,
        a: 0xff,
    };
    move |_theme, status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => lighten(accent, 0.15),
            _ => accent,
        };
        button::Style {
            background: Some(Background::Color(bg.iced_alpha(fade))),
            text_color: on_accent.iced_alpha(fade),
            border: Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// The glassy card: translucent fill, a gently breathing accent hairline, soft
/// drop shadow.
fn card_style(t: &Theme, fade: f32, phase: f32) -> impl Fn(&iced::Theme) -> container::Style {
    let card = t.card.iced_alpha(fade);
    let accent = t.accent;
    let radius = t.corner_radius;
    // The accent hairline breathes between ~0.18 and ~0.34 alpha.
    let breathe = 0.18 + 0.16 * (0.5 + 0.5 * (phase * 1.1).sin());
    move |_theme| container::Style {
        background: Some(Background::Color(card)),
        border: Border {
            radius: radius.into(),
            width: 1.0,
            color: accent.iced_alpha(fade * breathe),
        },
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45 * fade),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 34.0,
        },
        ..Default::default()
    }
}

/// The slim session selector, matched to the field styling.
fn picker_style(t: &Theme, fade: f32) -> impl Fn(&iced::Theme, pick_list::Status) -> pick_list::Style {
    let field = t.field.iced_alpha(fade);
    let fg = t.foreground.iced_alpha(fade);
    let muted = t.muted.iced_alpha(fade);
    let accent = t.accent.iced_alpha(fade);
    move |_theme, status| {
        let focused = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. });
        pick_list::Style {
            text_color: fg,
            placeholder_color: muted,
            handle_color: muted,
            background: Background::Color(field),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: if focused {
                    accent
                } else {
                    iced::Color::TRANSPARENT
                },
            },
        }
    }
}

/// A subtle, text-only power control that picks up the accent on hover.
fn power_button<'a>(
    label: &'a str,
    action: PowerAction,
    t: &Theme,
    fade: f32,
) -> Element<'a, Message> {
    let muted = t.muted.iced_alpha(fade);
    let accent = t.accent.iced_alpha(fade);
    button(text(label).size(12))
        .padding(8)
        .on_press(Message::PowerPressed(action))
        .style(move |_theme, status| button::Style {
            background: Some(Background::Color(iced::Color::TRANSPARENT)),
            text_color: if matches!(status, button::Status::Hovered) {
                accent
            } else {
                muted
            },
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// The subscription body: spawn the worker thread and stream its events. Created
/// once by Iced; `Subscription::run` keys it by this function so it is not
/// restarted on every frame.
fn daemon_worker() -> impl futures::Stream<Item = Message> {
    iced_futures::stream::channel(64, |mut output: futures::channel::mpsc::Sender<Message>| async move {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let thread_output = output.clone();
        std::thread::spawn(move || worker_loop(cmd_rx, thread_output));
        // Hand the UI its command channel; then idle forever while the worker
        // pushes events through its own clone of `output`.
        let _ = output.send(Message::WorkerReady(cmd_tx)).await;
        std::future::pending::<()>().await;
    })
}

/// The blocking worker: owns the protocol client and turns UI commands into
/// requests, emitting the daemon's responses back as [`Message`]s.
fn worker_loop(cmd_rx: mpsc::Receiver<Command>, mut out: futures::channel::mpsc::Sender<Message>) {
    let mut emit = |message: Message| {
        // Block the worker thread until Iced accepts the event (a tiny, bounded
        // wait); losing a prompt would hang the UI, so we never drop.
        let _ = futures::executor::block_on(out.send(message));
    };

    let socket = std::env::var("DOORD_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET.to_string());
    let mut client = match Client::connect(&socket) {
        Ok(client) => client,
        Err(e) => {
            emit(Message::Fatal(e.to_string()));
            return;
        }
    };

    match client.list_sessions() {
        Ok(sessions) => emit(Message::Sessions(sessions)),
        Err(e) => emit(Message::DaemonError(format!("Could not list sessions: {e}"))),
    }

    while let Ok(command) = cmd_rx.recv() {
        match command {
            Command::Authenticate { username } => {
                if let Err(e) = client.begin_auth(&username) {
                    emit(Message::DaemonError(e.to_string()));
                    continue;
                }
                run_conversation(&mut client, &cmd_rx, &mut emit);
            }
            Command::Start { session_id } => match client.start(&session_id) {
                Ok(StartOutcome::Started) => emit(Message::SessionStarted),
                Ok(StartOutcome::Refused(message)) => emit(Message::DaemonError(message)),
                Err(e) => emit(Message::Fatal(e.to_string())),
            },
            Command::Power(action) => match client.power(action) {
                // The daemon refused (e.g. not yet available); surface it.
                Ok(Some(message)) => emit(Message::DaemonError(message)),
                Ok(None) => {}
                Err(e) => emit(Message::DaemonError(e.to_string())),
            },
            // A stray reply outside a conversation: ignore.
            Command::Reply(_) => {}
        }
    }
}

/// Drive one authentication conversation: relay each PAM prompt as a [`Message`],
/// then block for the matching reply command, until a terminal verdict.
fn run_conversation(
    client: &mut Client,
    cmd_rx: &mpsc::Receiver<Command>,
    emit: &mut impl FnMut(Message),
) {
    loop {
        match client.recv_auth() {
            Ok(AuthStep::Question { text, secret }) => {
                emit(Message::Prompt { text, secret });
                // Wait for the UI to send the reply (or cancel).
                match cmd_rx.recv() {
                    Ok(Command::Reply(response)) => {
                        if let Err(e) = client.reply(response) {
                            emit(Message::DaemonError(e.to_string()));
                            return;
                        }
                    }
                    // The connection closed, or an out-of-band command: abort.
                    _ => return,
                }
            }
            Ok(AuthStep::Info(text)) | Ok(AuthStep::Error(text)) => emit(Message::Notice(text)),
            Ok(AuthStep::Success) => {
                emit(Message::AuthSucceeded);
                return;
            }
            Ok(AuthStep::Failure(reason)) => {
                emit(Message::AuthFailed(reason));
                return;
            }
            Err(e) => {
                emit(Message::Fatal(e.to_string()));
                return;
            }
        }
    }
}
