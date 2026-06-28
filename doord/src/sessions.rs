//! Discovery of startable desktop sessions.
//!
//! door must show the user which sessions they can log into and, on a successful
//! auth, start the one they chose. Both come from the freedesktop *session*
//! directories: `*.desktop` entries under `wayland-sessions/` and `xsessions/`
//! of each system data dir. Each describes a session with a human `Name`, an
//! optional `Comment`, and the `Exec=` command that actually starts it.
//!
//! This module owns that knowledge end to end: it scans the directories, parses
//! the entries, and yields a daemon-internal [`DiscoveredSession`]. The `Exec`
//! line **never leaves the daemon** — it is what the privileged core runs after
//! a login, so it is kept inside the trust boundary; the greeter only ever sees
//! the [`Session`] projection (`id`/`name`/`comment`). Handing the greeter the
//! command root will run would leak privileged detail for no benefit: the
//! greeter selects a session by `id`, never by command.
//!
//! Parsing is strict-but-tolerant: a single malformed `.desktop` file is skipped
//! with a log, never aborting discovery, because one bad package drop-in must not
//! make every session vanish from the login screen.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use protocol::Session;

/// Which display server a session speaks. Retained for the later seat/VT/logind
/// wiring (Wayland and X11 start differently); the greeter never sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Wayland,
    X11,
}

/// A session the daemon discovered and could start. Daemon-internal: unlike the
/// wire [`Session`], it carries the tokenized `Exec` the privileged core runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSession {
    /// Stable id — the `.desktop` basename without its extension.
    pub id: String,
    /// Human-facing name for the picker (`Name=`).
    pub name: String,
    /// Optional one-line description (`Comment=`).
    pub comment: Option<String>,
    /// The `Exec=` command, tokenized into program + arguments with desktop
    /// field codes stripped. Never crosses the seam; consumed by the spawn path.
    pub exec: Vec<String>,
    /// Wayland vs X11, from which subdirectory the entry was found in.
    pub kind: SessionKind,
}

impl DiscoveredSession {
    /// The greeter-facing projection: identity and display text only — no `Exec`.
    pub fn to_wire(&self) -> Session {
        Session {
            id: self.id.clone(),
            name: self.name.clone(),
            comment: self.comment.clone(),
        }
    }
}

/// The standard session subdirectories searched under each data-dir root, paired
/// with the display server they imply.
const SESSION_SUBDIRS: [(&str, SessionKind); 2] = [
    ("wayland-sessions", SessionKind::Wayland),
    ("xsessions", SessionKind::X11),
];

/// Default data-dir roots searched for the session subdirectories above.
///
/// The login manager runs before any user environment exists, so it cannot rely
/// on `XDG_DATA_DIRS` being set; these are the freedesktop default data dirs.
/// Overridable via `DOORD_SESSION_DIRS` (see `Config`) for testing and for
/// non-standard installs. Earlier roots take precedence on an id collision.
pub const DEFAULT_DATA_DIRS: [&str; 2] = ["/usr/local/share", "/usr/share"];

/// Discover every startable session under `data_dirs`, in precedence order.
///
/// For each root we look in `wayland-sessions/` and `xsessions/`; a missing
/// directory is normal and skipped silently. Entries are de-duplicated by `id`,
/// the first occurrence winning (so an admin override under `/usr/local/share`
/// shadows the packaged one under `/usr/share`). A malformed or unreadable entry
/// is logged and skipped, never fatal.
pub fn discover(data_dirs: &[PathBuf]) -> Vec<DiscoveredSession> {
    // X11 sessions need an X server started for them; door only hands off the seat to
    // a Wayland session (which is its own display server). Offering an X11 entry would
    // produce a login that can't succeed, so skip them unless explicitly allowed.
    discover_inner(data_dirs, std::env::var_os("DOORD_ALLOW_X11").is_some())
}

fn discover_inner(data_dirs: &[PathBuf], allow_x11: bool) -> Vec<DiscoveredSession> {
    let mut found = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut skipped_x11 = false;

    for root in data_dirs {
        for (subdir, kind) in SESSION_SUBDIRS {
            if matches!(kind, SessionKind::X11) && !allow_x11 {
                if root.join(subdir).is_dir() {
                    skipped_x11 = true;
                }
                continue;
            }
            let dir = root.join(subdir);
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                // A root without this subdir is the common case, not an error.
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                match parse_entry(&path, kind) {
                    Some(session) => {
                        if seen.insert(session.id.clone()) {
                            found.push(session);
                        } else {
                            eprintln!(
                                "doord: ignoring shadowed session '{}' at {} (already discovered)",
                                session.id,
                                path.display()
                            );
                        }
                    }
                    None => { /* parse_entry already logged why */ }
                }
            }
        }
    }

    if skipped_x11 {
        eprintln!(
            "doord: X11 sessions found but not offered — door starts no X server yet \
             (set DOORD_ALLOW_X11=1 to list them anyway)"
        );
    }

    found
}

/// Parse one `.desktop` file into a [`DiscoveredSession`], or `None` if it is
/// hidden, malformed, or missing the fields a startable session requires.
fn parse_entry(path: &Path, kind: SessionKind) -> Option<DiscoveredSession> {
    let id = path.file_stem().and_then(|s| s.to_str())?.to_string();

    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("doord: skipping unreadable session {}: {e}", path.display());
            return None;
        }
    };

    let entry = DesktopEntry::parse(&text);

    // An entry the packager marked away from view is not a login choice.
    if entry.boolean("Hidden") || entry.boolean("NoDisplay") {
        return None;
    }

    let name = match entry.value("Name") {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => {
            eprintln!("doord: skipping session {} (no Name=)", path.display());
            return None;
        }
    };

    let exec = match entry.value("Exec").map(tokenize_exec) {
        Some(argv) if !argv.is_empty() => argv,
        _ => {
            eprintln!("doord: skipping session {} (no runnable Exec=)", path.display());
            return None;
        }
    };

    let comment = entry
        .value("Comment")
        .filter(|c| !c.is_empty())
        .map(str::to_string);

    Some(DiscoveredSession {
        id,
        name,
        comment,
        exec,
        kind,
    })
}

/// The `[Desktop Entry]` group of a `.desktop` file, parsed into key→value.
///
/// We read only the first `[Desktop Entry]` group and ignore action groups and
/// the rest of the file. Localized keys (`Name[de]`) are skipped in favor of the
/// unlocalized key — door's greeter is not localized yet, and the C-locale value
/// is always present. Comment lines (`#…`) and blanks are ignored.
struct DesktopEntry {
    fields: Vec<(String, String)>,
}

impl DesktopEntry {
    fn parse(text: &str) -> Self {
        let mut fields = Vec::new();
        let mut in_group = false;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                // Entering a new group: we only care about the first Desktop Entry
                // group, and stop collecting once it ends.
                if in_group {
                    break;
                }
                in_group = line == "[Desktop Entry]";
                continue;
            }
            if !in_group {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                // Skip localized variants (`Name[de]`); take the unlocalized key.
                if key.contains('[') {
                    continue;
                }
                fields.push((key.to_string(), value.trim().to_string()));
            }
        }

        DesktopEntry { fields }
    }

    /// First value for `key`, if present.
    fn value(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// A desktop boolean: only the literal `true` is true.
    fn boolean(&self, key: &str) -> bool {
        self.value(key) == Some("true")
    }
}

/// Tokenize a desktop-entry `Exec=` value into an argv.
///
/// Implements the parts of the freedesktop Exec grammar that matter for a
/// session command: space-separated arguments, double-quoted arguments with
/// backslash escaping (`\"`, `` \` ``, `\$`, `\\`), and removal of field codes
/// (`%f`, `%U`, …) — sessions take no file/URL arguments, so a lone field code
/// is dropped and `%%` collapses to a literal `%`. Unterminated quotes yield
/// whatever was accumulated rather than failing, so a slightly malformed entry
/// still starts.
fn tokenize_exec(exec: &str) -> Vec<String> {
    let mut argv = Vec::new();
    let mut token = String::new();
    let mut have_token = false;
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if have_token {
                    push_token(&mut argv, std::mem::take(&mut token));
                    have_token = false;
                }
            }
            '"' => {
                have_token = true;
                while let Some(qc) = chars.next() {
                    match qc {
                        '"' => break,
                        '\\' => {
                            // Inside quotes, backslash escapes the reserved chars.
                            if let Some(&next) = chars.peek() {
                                if matches!(next, '"' | '`' | '$' | '\\') {
                                    token.push(next);
                                    chars.next();
                                    continue;
                                }
                            }
                            token.push('\\');
                        }
                        other => token.push(other),
                    }
                }
            }
            '%' => {
                // Field code: `%%` is a literal percent; any other `%x` is a code
                // a session command does not use, so drop it.
                have_token = true;
                if let Some('%') = chars.next() {
                    token.push('%');
                }
            }
            other => {
                have_token = true;
                token.push(other);
            }
        }
    }
    if have_token {
        push_token(&mut argv, token);
    }

    argv
}

/// Push a finished token, dropping it if stripping field codes left it empty
/// (a lone `%U` becomes nothing and must not appear as an empty argument).
fn push_token(argv: &mut Vec<String>, token: String) {
    if !token.is_empty() {
        argv.push(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A throwaway data-dir root with session subdirs, cleaned up on drop.
    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "doord-sesstest-{}-{}",
                std::process::id(),
                n
            ));
            std::fs::create_dir_all(path.join("wayland-sessions")).unwrap();
            std::fs::create_dir_all(path.join("xsessions")).unwrap();
            TempRoot { path }
        }

        fn write(&self, subdir: &str, file: &str, contents: &str) {
            std::fs::write(self.path.join(subdir).join(file), contents).unwrap();
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn discovers_wayland_and_x11_with_kind_and_metadata() {
        let root = TempRoot::new();
        root.write(
            "wayland-sessions",
            "hyprland.desktop",
            "[Desktop Entry]\nName=Hyprland\nComment=Dynamic tiler\nExec=Hyprland\n",
        );
        root.write(
            "xsessions",
            "i3.desktop",
            "[Desktop Entry]\nName=i3\nExec=/usr/bin/i3\n",
        );

        // With X11 allowed, both kinds parse with the right metadata.
        let mut sessions = discover_inner(std::slice::from_ref(&root.path), true);
        sessions.sort_by(|a, b| a.id.cmp(&b.id));

        assert_eq!(sessions.len(), 2);

        let hypr = sessions.iter().find(|s| s.id == "hyprland").unwrap();
        assert_eq!(hypr.name, "Hyprland");
        assert_eq!(hypr.comment.as_deref(), Some("Dynamic tiler"));
        assert_eq!(hypr.exec, vec!["Hyprland"]);
        assert_eq!(hypr.kind, SessionKind::Wayland);

        let i3 = sessions.iter().find(|s| s.id == "i3").unwrap();
        assert_eq!(i3.comment, None);
        assert_eq!(i3.exec, vec!["/usr/bin/i3"]);
        assert_eq!(i3.kind, SessionKind::X11);
    }

    #[test]
    fn x11_sessions_are_filtered_out_by_default() {
        let root = TempRoot::new();
        root.write(
            "wayland-sessions",
            "sway.desktop",
            "[Desktop Entry]\nName=Sway\nExec=sway\n",
        );
        root.write("xsessions", "i3.desktop", "[Desktop Entry]\nName=i3\nExec=i3\n");

        // The default (no DOORD_ALLOW_X11) offers only the Wayland session — door
        // starts no X server, so an X11 entry would be a login that can't succeed.
        let sessions = discover_inner(std::slice::from_ref(&root.path), false);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "sway");
        assert_eq!(sessions[0].kind, SessionKind::Wayland);
    }

    #[test]
    fn skips_hidden_nodisplay_and_fieldless_entries() {
        let root = TempRoot::new();
        root.write(
            "wayland-sessions",
            "hidden.desktop",
            "[Desktop Entry]\nName=Hidden\nExec=foo\nHidden=true\n",
        );
        root.write(
            "wayland-sessions",
            "nodisplay.desktop",
            "[Desktop Entry]\nName=Nope\nExec=foo\nNoDisplay=true\n",
        );
        root.write(
            "wayland-sessions",
            "noname.desktop",
            "[Desktop Entry]\nExec=foo\n",
        );
        root.write(
            "wayland-sessions",
            "noexec.desktop",
            "[Desktop Entry]\nName=NoExec\n",
        );
        root.write("wayland-sessions", "good.desktop", "[Desktop Entry]\nName=Good\nExec=good\n");

        let sessions = discover(std::slice::from_ref(&root.path));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "good");
    }

    #[test]
    fn earlier_root_shadows_later_on_id_collision() {
        let first = TempRoot::new();
        let second = TempRoot::new();
        first.write("wayland-sessions", "sway.desktop", "[Desktop Entry]\nName=Sway Override\nExec=sway-wrapped\n");
        second.write("wayland-sessions", "sway.desktop", "[Desktop Entry]\nName=Sway\nExec=sway\n");

        let sessions = discover(&[first.path.clone(), second.path.clone()]);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "Sway Override");
        assert_eq!(sessions[0].exec, vec!["sway-wrapped"]);
    }

    #[test]
    fn missing_directories_and_malformed_files_do_not_abort() {
        // A root that does not exist at all is fine.
        let bogus = PathBuf::from("/nonexistent-doord-data-dir");
        let root = TempRoot::new();
        root.write("wayland-sessions", "junk.desktop", "this is not\na desktop file\n");
        root.write("wayland-sessions", "ok.desktop", "[Desktop Entry]\nName=OK\nExec=ok\n");

        let sessions = discover(&[bogus, root.path.clone()]);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "ok");
    }

    #[test]
    fn ignores_localized_keys_and_later_groups() {
        let root = TempRoot::new();
        root.write(
            "wayland-sessions",
            "plasma.desktop",
            "[Desktop Entry]\nName=Plasma\nName[de]=Plasma-DE\nExec=startplasma-wayland\n\n[Desktop Action new]\nName=Should be ignored\nExec=should-not-win\n",
        );

        let sessions = discover(std::slice::from_ref(&root.path));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "Plasma");
        assert_eq!(sessions[0].exec, vec!["startplasma-wayland"]);
    }

    #[test]
    fn tokenize_handles_quotes_escapes_and_field_codes() {
        assert_eq!(tokenize_exec("env A=b start-hyprland"), vec!["env", "A=b", "start-hyprland"]);
        assert_eq!(tokenize_exec("gnome-session %U"), vec!["gnome-session"]);
        assert_eq!(
            tokenize_exec("\"/opt/My Session/run\" --flag"),
            vec!["/opt/My Session/run", "--flag"]
        );
        assert_eq!(tokenize_exec("echo 100%%done"), vec!["echo", "100%done"]);
        assert_eq!(tokenize_exec("   spaced    out   "), vec!["spaced", "out"]);
        assert!(tokenize_exec("   ").is_empty());
    }
}
