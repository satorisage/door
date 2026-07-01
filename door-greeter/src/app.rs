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
    button, canvas, column, container, image, pick_list, row, shader, stack, svg, text, text_input,
    Column, Space, Stack,
};
use iced::{
    keyboard, window, Alignment, Background, Border, ContentFit, Element, Font, Length, Shadow,
    Subscription, Task, Vector,
};

use protocol::{PowerAction, Secret, Session};

use crate::client::{AuthStep, Client, StartOutcome, DEFAULT_SOCKET};
use door_theme::clock::AnalogClock;
use door_theme::skyshader::{FrostShader, SkyShader, SpinnerShader};
use door_theme::{CardPos, ClockStyle, Color, Theme};

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

    // Render in the themed font + weight. A configured family must be installed
    // system-wide; iced wants a 'static name, so leak the single, process-lifetime
    // string. Weight applies whether or not a custom family is set.
    let mut font = match theme().font.clone() {
        Some(name) => Font::with_name(Box::leak(name.into_boxed_str())),
        None => Font::DEFAULT,
    };
    font.weight = theme().font_weight.iced();
    app = app.default_font(font);
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
        // Tab / Shift-Tab cycle focus; a CapsLock press re-reads the lock state.
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
            iced::Event::Keyboard(
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::CapsLock),
                    ..
                }
                | keyboard::Event::KeyReleased {
                    key: keyboard::Key::Named(keyboard::key::Named::CapsLock),
                    ..
                },
            ) => Some(Message::CapsLockChanged),
            _ => None,
        }),
    ];
    // Drive the animation off the compositor's frame clock (vsync — 60/120/144 Hz),
    // not a fixed-rate thread, so motion is buttery smooth. Only when animated. The GPU
    // level can cap the frame rate: capped tiers use a fixed-rate ticker,
    // `bonkers` (uncapped) rides vsync.
    if state.theme.animate {
        match state.theme.gpu_level.fps_cap() {
            Some(_) => subs.push(Subscription::run(frame_ticker)),
            None => subs.push(iced::window::frames().map(|_| Message::AnimTick)),
        }
    }
    Subscription::batch(subs)
}

/// Emit a [`Message::Tick`] once a second so the card clock stays current. A small
/// thread sleeps and pushes through the Iced channel (same shape as the daemon
/// worker) — avoids pulling an async timer/runtime feature onto the greeter.
fn clock_ticker() -> impl futures::Stream<Item = Message> {
    iced_futures::stream::channel(
        4,
        |output: futures::channel::mpsc::Sender<Message>| async move {
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
        },
    )
}

/// Drive the launch fade-in: ~24 [`Message::Fade`] ticks over ~380 ms, then the
/// stream ends (bounded — no perpetual repaint on the idle greeter).
fn fade_ticker() -> impl futures::Stream<Item = Message> {
    // ~16 ms steps over the configured fade duration (one extra step lands fade at 1.0).
    let ticks = (theme().fade_ms / 16.0).ceil() as usize + 1;
    iced_futures::stream::channel(
        4,
        move |output: futures::channel::mpsc::Sender<Message>| async move {
            std::thread::spawn(move || {
                let mut output = output;
                for _ in 0..ticks {
                    std::thread::sleep(Duration::from_millis(16));
                    if futures::executor::block_on(output.send(Message::Fade)).is_err() {
                        return;
                    }
                }
            });
            std::future::pending::<()>().await;
        },
    )
}

/// Emit [`Message::AnimTick`] at a fixed frame cap (the global GPU level) instead of the
/// uncapped vsync `window::frames()`. `anim` is recomputed from real elapsed time each
/// tick, so motion stays time-correct — we just repaint (and re-run the sky shader)
/// fewer times per second. A small sleeping thread, same shape as `clock_ticker`.
fn frame_ticker() -> impl futures::Stream<Item = Message> {
    let fps = theme().gpu_level.fps_cap().unwrap_or(60);
    let dt = Duration::from_micros(1_000_000 / fps.max(1) as u64);
    iced_futures::stream::channel(
        4,
        move |output: futures::channel::mpsc::Sender<Message>| async move {
            std::thread::spawn(move || {
                let mut output = output;
                loop {
                    std::thread::sleep(dt);
                    if futures::executor::block_on(output.send(Message::AnimTick)).is_err() {
                        break;
                    }
                }
            });
            std::future::pending::<()>().await;
        },
    )
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
    Prompt {
        text: String,
        secret: bool,
    },
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
    /// CapsLock was pressed/released — re-read the lock state.
    CapsLockChanged,
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
    /// Whether `status` is an error (login failed, daemon unreachable) — colored with
    /// the theme's `error_color` rather than the muted tone.
    status_error: bool,
    cmd_tx: Option<mpsc::Sender<Command>>,
    /// The resolved look (a clone of the process-wide [`theme`]).
    theme: Theme,
    /// The theme's wallpaper/logo paths *after* the pre-auth trust + size vetting
    /// ([`vet_theme_assets`]) — `None` where the configured asset was refused. Vetted
    /// once when the theme is set, so `view` never re-stats the filesystem per frame.
    wallpaper: Option<std::path::PathBuf>,
    logo: Option<std::path::PathBuf>,
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
    /// Whether Caps Lock is currently on — read from the keyboard LED (local; no
    /// daemon). Drives the warning shown by the password field.
    caps_lock: bool,
}

impl State {
    fn new() -> Self {
        // Pick the day or night variant by the local clock — the greeter runs
        // pre-login, so it can't read the user's color scheme; time is the trigger.
        let theme = Theme::load_at(now_minutes(), now_month());
        let clock = now_hm(
            theme.clock_24h,
            theme.clock_seconds,
            theme.clock_format.as_deref(),
        );
        let (wallpaper, logo) = vet_theme_assets(&theme);
        State {
            phase: Phase::Connecting,
            sessions: Vec::new(),
            selected: None,
            username: String::new(),
            password: String::new(),
            status: "Connecting to doord…".to_string(),
            status_error: false,
            cmd_tx: None,
            theme,
            wallpaper,
            logo,
            clock,
            date: now_date(),
            fade: 0.0,
            anim: 0.0,
            started: Instant::now(),
            caps_lock: caps_lock_on().unwrap_or(false),
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
            state.status_error = true;
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
            state.status_error = true;
            state.phase = Phase::Ready;
        }
        Message::Fatal(message) => {
            state.status = format!("Cannot reach doord: {message}");
            state.status_error = true;
            state.phase = Phase::Connecting;
        }
        Message::UsernameChanged(value) => state.username = value,
        Message::PasswordChanged(value) => state.password = value,
        Message::SessionPicked(choice) => state.selected = Some(choice),
        Message::LoginPressed => {
            state.status_error = false;
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
            // Live config hot-reload: re-resolve the theme each second and swap it only
            // if it changed — so an edit to greeter.toml (or crossing the day/night
            // boundary) takes effect without restarting the greeter. Cheap: one small
            // TOML parse, and the PartialEq guard avoids needless repaint churn.
            let fresh = Theme::load_at(now_minutes(), now_month());
            if fresh != state.theme {
                // Re-vet assets only when the theme actually changed (a config edit or a
                // day/night flip) — not every tick.
                let (wallpaper, logo) = vet_theme_assets(&fresh);
                state.wallpaper = wallpaper;
                state.logo = logo;
                state.theme = fresh;
            }
            state.clock = now_hm(
                state.theme.clock_24h,
                state.theme.clock_seconds,
                state.theme.clock_format.as_deref(),
            );
            state.date = now_date();
            // Backstop poll of the Caps Lock LED, in case a press arrived while the
            // greeter lacked focus (the key-event path catches the focused case).
            if let Some(on) = caps_lock_on() {
                state.caps_lock = on;
            }
        }
        Message::Fade => state.fade = (state.fade + 16.0 / state.theme.fade_ms.max(16.0)).min(1.0),
        // Recompute the clock from real elapsed time each frame — smooth, jitter-free.
        Message::AnimTick => state.anim = state.started.elapsed().as_secs_f32() % 10_000.0,
        Message::FocusNext => task = iced::widget::operation::focus_next(),
        Message::FocusPrev => task = iced::widget::operation::focus_previous(),
        Message::CapsLockChanged => {
            if let Some(on) = caps_lock_on() {
                state.caps_lock = on;
            }
        }
    }
    task
}

/// Whether a logo path is an SVG (case-insensitive `.svg`) — chooses the vector
/// renderer over the raster one.
fn is_svg(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

/// The only directories the pre-auth greeter loads wallpaper/logo assets from —
/// the same root-owned trees door's own config is read from. A local unprivileged
/// user cannot write here, so an asset resolved into one of these is trustworthy;
/// a path anywhere else (a world-writable `/tmp`, a user's home) could be swapped
/// out from under the login screen before authentication, so it is refused.
const ASSET_ROOTS: [&str; 2] = ["/usr/share/door", "/etc/door"];

/// Decompression-bomb guard for raster assets: a decoded frame larger than this in
/// either dimension, or above the pixel budget, is refused rather than handed to the
/// image decoder (which would allocate for the full frame). 8K is ~33 MP, so this
/// clears any real display wallpaper while rejecting a hostile 100k×100k PNG.
const MAX_ASSET_DIM: u32 = 8192;
const MAX_ASSET_PIXELS: u64 = 40_000_000;

/// Vet a configured asset before the greeter loads it, pre-auth. Returns the
/// canonical path if it is trusted, or `None` (with a logged explanation) if it is
/// refused — the caller then renders without it (a solid background / no logo).
///
/// Two gates: the path must resolve *inside* [`ASSET_ROOTS`] (canonicalized first,
/// so a symlink pointing out of the trusted tree is caught), and a raster asset must
/// decode within the size cap. A `DOORD_GREETER_CONFIG` dev/test config is an
/// explicit trusted-operator signal and may load assets from anywhere — but the size
/// cap still applies.
fn vet_asset(path: &std::path::Path, kind: &str) -> Option<std::path::PathBuf> {
    let real = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "door: refusing {kind} {}: cannot resolve the path ({e}); loading without it.",
                path.display()
            );
            return None;
        }
    };

    let dev = std::env::var_os(door_theme::ENV_CONFIG).is_some();
    if !dev && !ASSET_ROOTS.iter().any(|root| real.starts_with(root)) {
        eprintln!(
            "door: refusing {kind} {}: pre-auth assets may only be loaded from {}. \
             A path outside those root-owned directories could be replaced by a local \
             user before login, so it is not loaded — the greeter falls back to none.",
            real.display(),
            ASSET_ROOTS.join(" or "),
        );
        return None;
    }

    // SVG has no meaningful raster dimensions; the trusted-path gate above is its
    // defense (and it is the higher-risk parser, so it is only ever loaded from root).
    // Probe only the header — `into_dimensions` reads the size without decoding the
    // full frame, so the bomb never gets allocated.
    if !is_svg(&real) {
        // `::image` = the `image` crate (the bare `image` in this module is the iced
        // widget, which is imported and shadows it).
        let dims = ::image::ImageReader::open(&real)
            .map_err(|e| e.to_string())
            .and_then(|r| r.with_guessed_format().map_err(|e| e.to_string()))
            .and_then(|r| r.into_dimensions().map_err(|e| e.to_string()));
        match dims {
            Ok((w, h))
                if w > MAX_ASSET_DIM
                    || h > MAX_ASSET_DIM
                    || (w as u64 * h as u64) > MAX_ASSET_PIXELS =>
            {
                eprintln!(
                    "door: refusing {kind} {}: {w}×{h} exceeds the {MAX_ASSET_DIM}px / {} MP \
                     asset cap (a decompression-bomb guard); loading without it.",
                    real.display(),
                    MAX_ASSET_PIXELS / 1_000_000,
                );
                return None;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "door: refusing {kind} {}: not a decodable image ({e}); loading without it.",
                    real.display()
                );
                return None;
            }
        }
    }

    Some(real)
}

/// Vet a theme's wallpaper + logo once (when it is loaded or hot-reloaded), so the
/// per-frame `view` never re-stats or re-probes. Returns the trusted paths to render.
fn vet_theme_assets(theme: &Theme) -> (Option<std::path::PathBuf>, Option<std::path::PathBuf>) {
    let wallpaper = theme
        .wallpaper
        .as_ref()
        .and_then(|p| vet_asset(p, "wallpaper"));
    let logo = theme.logo.as_ref().and_then(|p| vet_asset(p, "logo"));
    (wallpaper, logo)
}

/// Whether Caps Lock is on, from the keyboard's `capslock` LED under
/// `/sys/class/leds/*::capslock/brightness` (e.g. `input3::capslock`). A purely
/// local read — no daemon, no privileged path, and it discloses nothing sensitive.
/// `None` when no such LED exists (e.g. some laptops) so the caller can hide the
/// hint rather than assert a state it cannot know.
fn caps_lock_on() -> Option<bool> {
    let entries = std::fs::read_dir("/sys/class/leds").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with("::capslock") {
            let brightness = std::fs::read_to_string(entry.path().join("brightness")).ok()?;
            return Some(brightness.trim() != "0");
        }
    }
    None
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

/// Local wall-clock time. A `format` (a `strftime` string) wins when set; otherwise
/// the built-in `HH:MM` (24-hour) / `H:MM AM/PM` (12-hour), with optional seconds.
fn now_hm(clock_24h: bool, seconds: bool, format: Option<&str>) -> String {
    let Some(tm) = local_tm() else {
        return String::new();
    };
    if let Some(fmt) = format {
        return strftime(&tm, fmt);
    }
    if clock_24h {
        if seconds {
            format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
        } else {
            format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
        }
    } else {
        let h12 = match tm.tm_hour % 12 {
            0 => 12,
            h => h,
        };
        let meridiem = if tm.tm_hour < 12 { "AM" } else { "PM" };
        if seconds {
            format!("{}:{:02}:{:02} {}", h12, tm.tm_min, tm.tm_sec, meridiem)
        } else {
            format!("{}:{:02} {}", h12, tm.tm_min, meridiem)
        }
    }
}

/// Format a `tm` via the C library's `strftime`. Empty on a malformed format (an
/// interior NUL) or if the result overflows the buffer — the caller keeps the last
/// good clock. No date/time crate on the pre-auth surface.
fn strftime(tm: &libc::tm, fmt: &str) -> String {
    let Ok(cfmt) = std::ffi::CString::new(fmt) else {
        return String::new();
    };
    let mut buf = [0u8; 128];
    // SAFETY: strftime writes at most buf.len() bytes (including the NUL) into buf,
    // and only reads the borrowed `tm` and our NUL-terminated format string.
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            cfmt.as_ptr(),
            tm,
        )
    };
    if n == 0 {
        return String::new();
    }
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

/// Local time as minutes since midnight (for the day/night window). Noon on failure.
fn now_minutes() -> u32 {
    match local_tm() {
        Some(tm) => (tm.tm_hour.clamp(0, 23) as u32) * 60 + (tm.tm_min.clamp(0, 59) as u32),
        None => 12 * 60,
    }
}

/// Local calendar month, 1–12 (for the seasonal scene selector). June on failure.
fn now_month() -> u32 {
    match local_tm() {
        Some(tm) => (tm.tm_mon.clamp(0, 11) as u32) + 1,
        None => 6,
    }
}

/// Local date as `Weekday, Month D` (e.g. `Friday, June 27`).
fn now_date() -> String {
    const DAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
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

/// Clock-hand angles (radians, clockwise from 12 o'clock) for the analog face.
/// Seconds carry a sub-second fraction from `gettimeofday`, so the second hand
/// sweeps smoothly when the greeter is animating (and ticks at 1 Hz when it isn't).
fn clock_angles() -> (f32, f32, f32) {
    let Some(tm) = local_tm() else {
        return (0.0, 0.0, 0.0);
    };
    // Sub-second fraction for the smooth sweep. SAFETY: fills our owned timeval.
    let frac = unsafe {
        let mut tv: libc::timeval = std::mem::zeroed();
        if libc::gettimeofday(&mut tv, std::ptr::null_mut()) == 0 {
            (tv.tv_usec as f32) / 1_000_000.0
        } else {
            0.0
        }
    };
    let tau = std::f32::consts::TAU;
    let sec = tm.tm_sec as f32 + frac;
    let min = tm.tm_min as f32 + sec / 60.0;
    let hour = (tm.tm_hour % 12) as f32 + min / 60.0;
    (hour / 12.0 * tau, min / 60.0 * tau, sec / 60.0 * tau)
}

fn view(state: &State) -> Element<'_, Message> {
    let t = &state.theme;
    let f = state.fade.clamp(0.0, 1.0);
    let fg = t.foreground.iced_alpha(f);
    let muted = t.muted.iced_alpha(f);

    // Clock + date — the minimal focal point at the top of the card.
    let header: Element<Message> = if t.show_clock {
        let time_widget: Element<Message> = match t.clock_style {
            ClockStyle::Digital => text(state.clock.clone())
                .size(t.clock_size * t.font_scale)
                .color(fg)
                .into(),
            ClockStyle::Analog => {
                let d = t.clock_size * t.font_scale * 2.3;
                canvas(AnalogClock::new(t, f, clock_angles()))
                    .width(Length::Fixed(d))
                    .height(Length::Fixed(d))
                    .into()
            }
        };
        column![
            time_widget,
            text(state.date.clone())
                .size(13.0 * t.font_scale)
                .color(muted),
        ]
        .spacing(2)
        .align_x(Alignment::Center)
        .into()
    } else {
        Space::new().into()
    };

    // Emblem: a user-set image (SVG crisp, else raster) overrides; otherwise the
    // animated spinner — unless the style is `none`, which shows no emblem at all.
    let emblem: Option<Element<Message>> = match &state.logo {
        Some(path) if is_svg(path) => Some(
            svg(svg::Handle::from_path(path.clone()))
                .height(Length::Fixed(56.0))
                .into(),
        ),
        Some(path) => Some(
            image(image::Handle::from_path(path))
                .height(Length::Fixed(56.0))
                .into(),
        ),
        None if t.spinner_style.is_hidden() => None,
        None => Some(
            shader(SpinnerShader::from_theme(t, state.anim, f))
                .width(Length::Fixed(t.spinner_size))
                .height(Length::Fixed(t.spinner_size))
                .into(),
        ),
    };
    // Wrap in the optional backdrop tile (transparent by default); `None` → no emblem.
    let logo_bg = t.logo_box.iced_alpha(f);
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

    let username = text_input("user", &state.username)
        .on_input(Message::UsernameChanged)
        .on_submit(Message::LoginPressed)
        .padding(11)
        .size(15.0 * t.font_scale)
        .style(field_style(t, f));

    let password = text_input("password", &state.password)
        .on_input(Message::PasswordChanged)
        .on_submit(Message::LoginPressed)
        .secure(true)
        .padding(11)
        .size(15.0 * t.font_scale)
        .style(field_style(t, f));

    let busy = matches!(state.phase, Phase::Authenticating | Phase::Started);
    let mut login = button(
        text("Sign in")
            .width(Length::Fill)
            .center()
            .size(15.0 * t.font_scale),
    )
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
    .text_size(13.0 * t.font_scale)
    .padding(8)
    .width(Length::Fill)
    .style(picker_style(t, f));

    // Status only takes space when there is something to say.
    let status: Element<Message> = if state.status.is_empty() {
        Space::new().into()
    } else {
        let tone = if state.status_error {
            t.error_color.iced_alpha(f)
        } else {
            muted
        };
        text(state.status.clone())
            .size(12.0 * t.font_scale)
            .color(tone)
            .into()
    };

    // A Caps Lock warning slips in under the password field only while it's on, so
    // there's no empty gap otherwise. Standard login-screen courtesy.
    let mut items: Vec<Element<Message>> = vec![header];
    if let Some(logo) = logo {
        items.push(logo);
    }
    items.push(username.into());
    items.push(password.into());
    if state.caps_lock {
        items.push(
            text("⇪  Caps Lock is on")
                .size(12.0 * t.font_scale)
                .color(t.error_color.iced_alpha(f))
                .into(),
        );
    }
    items.push(login.into());
    items.push(picker.into());
    items.push(status);
    let form = Column::with_children(items)
        .spacing(12)
        .align_x(Alignment::Center);

    let card = container(form)
        .padding(26)
        .width(Length::Fixed(t.card_width))
        .style(card_style(t, f, state.anim));

    // Optional true backdrop blur: a frosted sample of the sky behind the card, drawn
    // *under* the translucent card (push_under keeps the card the size-defining base)
    // and masked to its rounded rect inside the shader. The GPU level can veto
    // this second full-screen pass on lower tiers, even if the user enabled it.
    let card: Element<Message> = if t.card_blur && t.gpu_level.allows_blur() {
        Stack::new()
            .push(card)
            .push_under(
                shader(FrostShader::from_theme(t, state.anim, f))
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .into()
    } else {
        card.into()
    };

    // Place the card per the theme; padding keeps edge placements off the bezel.
    let card_area = container(card).padding(48);
    let centered = match t.card_pos {
        CardPos::Center => card_area.center_x(Length::Fill).center_y(Length::Fill),
        CardPos::Left => card_area.align_left(Length::Fill).center_y(Length::Fill),
        CardPos::Right => card_area.align_right(Length::Fill).center_y(Length::Fill),
        CardPos::Top => card_area.center_x(Length::Fill).align_top(Length::Fill),
        CardPos::Bottom => card_area.center_x(Length::Fill).align_bottom(Length::Fill),
    };

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
        let sky = shader(SkyShader::from_theme(t, state.anim, f))
            .width(Length::Fill)
            .height(Length::Fill);
        stack![sky, overlay].into()
    } else {
        overlay.into()
    };

    // Wallpaper behind everything, if configured and it passed the pre-auth asset
    // vetting (a missing/refused file just leaves the solid window background).
    match &state.wallpaper {
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
fn field_style(
    t: &Theme,
    fade: f32,
) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    let field = t.field.iced_alpha(fade);
    let fg = t.foreground.iced_alpha(fade);
    let muted = t.muted.iced_alpha(fade);
    let accent = t.accent.iced_alpha(fade);
    let radius = t.field_radius;
    move |_theme, status| {
        let focused = matches!(status, text_input::Status::Focused { .. });
        text_input::Style {
            background: Background::Color(field),
            border: Border {
                radius: radius.into(),
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
    let radius = t.field_radius;
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
                radius: radius.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// The glassy card: translucent fill, a gently breathing accent hairline, soft
/// drop shadow.
/// The card fill: a flat color, or — when `gradient` > 0 — a subtle vertical sheen
/// whose top edge lightens toward white and eases down to the card color.
fn card_background(card: iced::Color, gradient: f32) -> Background {
    if gradient <= 0.0 {
        return Background::Color(card);
    }
    let amt = 0.18 * gradient.clamp(0.0, 1.0);
    let lit = |c: f32| c + (1.0 - c) * amt;
    let top = iced::Color {
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

fn card_style(t: &Theme, fade: f32, phase: f32) -> impl Fn(&iced::Theme) -> container::Style {
    let card = t.card.iced_alpha(fade);
    let accent = t.accent;
    let radius = t.corner_radius;
    let shadow_opacity = t.card_shadow_opacity;
    let shadow_blur = t.card_shadow_blur;
    let gradient = t.card_gradient;
    // The accent hairline breathes between ~0.18 and ~0.34 alpha (speed × control).
    let breathe = 0.18 + 0.16 * (0.5 + 0.5 * (phase * 1.1 * t.accent_breathing).sin());
    move |_theme| container::Style {
        background: Some(card_background(card, gradient)),
        border: Border {
            radius: radius.into(),
            width: 1.0,
            color: accent.iced_alpha(fade * breathe),
        },
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, shadow_opacity * fade),
            offset: Vector::new(0.0, 10.0),
            blur_radius: shadow_blur,
        },
        ..Default::default()
    }
}

/// The slim session selector, matched to the field styling.
fn picker_style(
    t: &Theme,
    fade: f32,
) -> impl Fn(&iced::Theme, pick_list::Status) -> pick_list::Style {
    let field = t.field.iced_alpha(fade);
    let fg = t.foreground.iced_alpha(fade);
    let muted = t.muted.iced_alpha(fade);
    let accent = t.accent.iced_alpha(fade);
    let radius = t.field_radius;
    move |_theme, status| {
        let focused = matches!(
            status,
            pick_list::Status::Hovered | pick_list::Status::Opened { .. }
        );
        pick_list::Style {
            text_color: fg,
            placeholder_color: muted,
            handle_color: muted,
            background: Background::Color(field),
            border: Border {
                radius: radius.into(),
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
    iced_futures::stream::channel(
        64,
        |mut output: futures::channel::mpsc::Sender<Message>| async move {
            let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
            let thread_output = output.clone();
            std::thread::spawn(move || worker_loop(cmd_rx, thread_output));
            // Hand the UI its command channel; then idle forever while the worker
            // pushes events through its own clone of `output`.
            let _ = output.send(Message::WorkerReady(cmd_tx)).await;
            std::future::pending::<()>().await;
        },
    )
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
        Err(e) => emit(Message::DaemonError(format!(
            "Could not list sessions: {e}"
        ))),
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

#[cfg(test)]
mod asset_vetting_tests {
    use super::vet_asset;
    use std::path::Path;

    /// A perfectly valid image that happens to live outside the trusted roots
    /// (`/tmp`) must be refused pre-auth — a local user could have written it.
    #[test]
    fn refuses_a_valid_image_outside_the_trusted_roots() {
        // SAFETY: single-threaded test; no other test touches this var. Ensure we are
        // in production mode (no dev-config bypass) so the allowlist is enforced.
        unsafe { std::env::remove_var(door_theme::ENV_CONFIG) };
        let p = std::env::temp_dir().join(format!("door-vet-{}.png", std::process::id()));
        ::image::RgbaImage::new(2, 2)
            .save(&p)
            .expect("write test png");
        assert!(
            vet_asset(&p, "wallpaper").is_none(),
            "a valid image under /tmp must be refused by the trusted-roots allowlist"
        );
        let _ = std::fs::remove_file(&p);
    }

    /// A path that does not resolve is refused (canonicalize fails) — never loaded.
    #[test]
    fn refuses_a_nonexistent_asset() {
        assert!(vet_asset(Path::new("/door/nope/missing.png"), "logo").is_none());
    }
}
