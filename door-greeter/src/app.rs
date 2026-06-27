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
use std::time::Duration;

use futures::SinkExt;
use iced::widget::{
    button, column, container, image, pick_list, row, stack, text, text_input, Space,
};
use iced::{window, Alignment, Background, Border, ContentFit, Element, Length, Subscription, Task};

use protocol::{PowerAction, Secret, Session};

use crate::client::{AuthStep, Client, StartOutcome, DEFAULT_SOCKET};
use crate::theme::Theme;

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

    iced::application(State::new, update, view)
        .title("door")
        .window(window::Settings {
            fullscreen,
            ..Default::default()
        })
        .style(app_style)
        .subscription(subscription)
        .run()
}

/// Window-level appearance from the theme: the solid background (also shown at any
/// wallpaper letterbox edge) and the default text color.
fn app_style(state: &State, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: state.theme.background.iced(),
        text_color: state.theme.foreground.iced(),
    }
}

/// The greeter's subscriptions: the daemon worker event stream plus a 1 Hz clock.
fn subscription(_state: &State) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(daemon_worker),
        Subscription::run(clock_ticker),
    ])
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
    /// 1 Hz clock tick — refreshes the card clock.
    Tick,
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
    /// The resolved look, loaded once at startup (M4).
    theme: Theme,
    /// Current local time, `HH:MM`, refreshed by [`Message::Tick`].
    clock: String,
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
            theme: Theme::load(),
            clock: now_hm(),
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
        Message::Tick => state.clock = now_hm(),
    }
    task
}

/// Local wall-clock time as `HH:MM`, without pulling a date/time crate onto the
/// pre-auth surface: `localtime_r` on the current epoch second.
fn now_hm() -> String {
    // SAFETY: `time(NULL)` returns the epoch seconds; `localtime_r` fills a caller-
    // owned `tm` from it (no shared state, no allocation). Both are async-signal-safe
    // libc calls; here they just read the clock.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    }
}

fn view(state: &State) -> Element<'_, Message> {
    let t = &state.theme;
    let fg = t.foreground.iced();
    let muted = t.muted.iced();
    let accent = t.accent.iced();

    let clock: Element<Message> = if t.show_clock {
        text(state.clock.clone()).size(30).color(muted).into()
    } else {
        Space::new().into()
    };

    let logo: Element<Message> = match &t.logo {
        Some(path) => image(image::Handle::from_path(path))
            .height(Length::Fixed(72.0))
            .into(),
        None => Space::new().into(),
    };

    let title = text("door").size(44).color(fg);

    let picker = pick_list(
        state.sessions.clone(),
        state.selected.clone(),
        Message::SessionPicked,
    )
    .placeholder("Session")
    .width(Length::Fill);

    let username = text_input("Username", &state.username)
        .on_input(Message::UsernameChanged)
        .on_submit(Message::LoginPressed)
        .padding(10);

    let password = text_input("Password", &state.password)
        .on_input(Message::PasswordChanged)
        .on_submit(Message::LoginPressed)
        .secure(true)
        .padding(10);

    let busy = matches!(state.phase, Phase::Authenticating | Phase::Started);
    let mut login = button(text("Sign in"))
        .padding(10)
        .width(Length::Fill)
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(accent)),
            text_color: iced::Color::WHITE,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    if !busy {
        login = login.on_press(Message::LoginPressed);
    }

    let power = row![
        button(text("Suspend")).on_press(Message::PowerPressed(PowerAction::Suspend)),
        button(text("Reboot")).on_press(Message::PowerPressed(PowerAction::Reboot)),
        button(text("Power off")).on_press(Message::PowerPressed(PowerAction::PowerOff)),
    ]
    .spacing(10);

    let form = column![
        clock,
        logo,
        title,
        picker,
        username,
        password,
        login,
        text(state.status.clone()).color(muted),
        power,
    ]
    .spacing(14)
    .align_x(Alignment::Center);

    // The frosted card: a translucent, rounded panel holding the form.
    let card_color = t.card.iced();
    let radius = t.corner_radius;
    let card = container(form)
        .padding(28)
        .max_width(t.card_width)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(card_color)),
            border: Border {
                radius: radius.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let centered = container(card).center_x(Length::Fill).center_y(Length::Fill);

    // Wallpaper behind the card when one is configured (a missing file just renders
    // nothing here, leaving the solid window background from `app_style`).
    match &t.wallpaper {
        Some(path) => {
            let background = image(image::Handle::from_path(path))
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Cover);
            stack![background, centered].into()
        }
        None => centered.into(),
    }
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
