//! door-lock — the session lock screen.
//!
//! An `ext-session-lock-v1` client of the *user's running compositor* that
//! renders the door-theme surface and unlocks by asking doord (over the
//! dedicated reauth socket) to verify this uid's own credentials. It is not
//! the greeter: it runs *inside* an authenticated session, the compositor —
//! not door — owns the lock surfaces and the blanking, and its failure mode
//! is "stays locked" (recover from a TTY), never "reveals the session."
//!
//! Requires a compositor with `ext-session-lock-v1` (sway, Hyprland, river,
//! niri, labwc, Wayfire, COSMIC, Weston 12+, …). KWin/Plasma and GNOME are not
//! supported — both ship their own built-in lockers and do not expose the
//! protocol to third-party clients — and door-lock declines cleanly rather
//! than shipping an overlay fallback that would lose the crash-safety
//! guarantee.

mod app;
mod client;

const USAGE: &str = "\
door-lock — lock the current Wayland session behind the door surface

Usage: door-lock [--help] [--version]

The compositor must support ext-session-lock-v1 (sway, Hyprland, river, niri,
labwc, Wayfire, COSMIC, Weston 12+, ...). KWin/Plasma and GNOME ship their own
lockers and do not accept third-party ones — door-lock declines there. Unlock
verifies your own credentials via doord's reauth socket; there is no way to
unlock as, or probe, another user.

Environment:
  DOORD_REAUTH_SOCKET  reauth socket path (default /run/doord-reauth/reauth.sock)
  DOOR_LOCK_DEV        run as a plain window (no lock protocol) for testing

Recovery if the locker wedges (screen stays blanked by the compositor):
  switch to a TTY (Ctrl+Alt+F3), log in, then:
    pkill door-lock; door-lock &
  (run in your session's environment, e.g. via systemd-run --user or from
   the TTY with WAYLAND_DISPLAY/XDG_RUNTIME_DIR of the locked session)";

fn main() {
    // Every recognized argument terminates, so only the first can matter.
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            "--version" | "-V" => {
                println!("door-lock {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("door-lock: unknown argument {other:?}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    if let Err(e) = app::run() {
        eprintln!("door-lock: {e}");
        std::process::exit(1);
    }
}
