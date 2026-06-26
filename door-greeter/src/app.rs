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

use futures::SinkExt;
use iced::widget::{button, column, container, pick_list, row, text, text_input};
use iced::{window, Alignment, Element, Length, Task};

use protocol::{PowerAction, Secret, Session};

use crate::client::{AuthStep, Client, StartOutcome, DEFAULT_SOCKET};

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
        .subscription(|_state| iced_futures::Subscription::run(daemon_worker))
        .run()
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
    }
    Task::none()
}

fn view(state: &State) -> Element<'_, Message> {
    let title = text("door").size(48);

    let picker = pick_list(
        state.sessions.clone(),
        state.selected.clone(),
        Message::SessionPicked,
    )
    .placeholder("Session");

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
    let mut login = button(text("Sign in"));
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
        title,
        picker,
        username,
        password,
        login,
        text(&state.status),
        power,
    ]
    .spacing(14)
    .align_x(Alignment::Center)
    .max_width(360);

    container(form)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
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
