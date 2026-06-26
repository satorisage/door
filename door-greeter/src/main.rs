//! door-greeter — the unprivileged greeter UI.
//!
//! This is the beautiful half and the untrusted half. It runs as a system user
//! with no human home directory, owns no privilege, and holds no authority
//! beyond connecting to the daemon's socket and forwarding what the user types.
//! It collects credentials and hands them straight to the daemon for the PAM
//! conversation; it never validates them, never starts a session, never touches
//! a seat. If this process is compromised, the attacker is still not root —
//! that separation is the whole point.
//!
//! Because it runs pre-session, every asset it renders (theme, cursor, font,
//! shaders, wallpaper) must be installed system-wide and world-readable, or it
//! silently falls back to stock. The rendering toolkit is not yet chosen; this
//! skeleton is the protocol-facing shell that a UI will be built onto.

fn main() {
    // Skeleton. The Wayland surface, session picker, and the daemon connection
    // (speaking the `protocol` crate's messages over the local socket) land
    // here once the privileged core can answer them.
    eprintln!("door-greeter: skeleton — UI not yet implemented");
}
