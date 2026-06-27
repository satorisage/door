//! The greeter's look (M4): a config-driven theme with a built-in beautiful
//! default. The greeter is the pre-auth surface, so the config is admin-controlled
//! and world-readable — never user-supplied at login time.
//!
//! Loaded at startup from the first file that exists, each merged *over* the
//! built-in [`Theme::default`] so a partial file (just an `accent`, say) works:
//!
//! 1. `$DOORD_GREETER_CONFIG` — explicit path, for dev/testing
//! 2. `/etc/door/greeter.toml` — admin override
//! 3. `/usr/share/door/greeter.toml` — packaged default
//!
//! If none exist or one fails to parse, the built-in default stands (the greeter
//! must always render). Asset paths (wallpaper, font, logo) point at world-readable
//! files installed system-wide; a missing asset degrades to the solid background /
//! stock font rather than failing.

use serde::Deserialize;
use std::path::PathBuf;

/// Explicit config path override (dev/testing). Checked before the system paths.
const ENV_CONFIG: &str = "DOORD_GREETER_CONFIG";
/// Admin override, then the packaged default. First readable wins.
const SYSTEM_CONFIG_PATHS: [&str; 2] = ["/etc/door/greeter.toml", "/usr/share/door/greeter.toml"];

/// An 8-bit RGBA color, parsed from `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 0xff }
    }
    const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Parse `#rrggbb` or `#rrggbbaa` (case-insensitive). Returns `None` on any
    /// malformed input so the caller can keep the default and warn.
    fn parse(s: &str) -> Option<Color> {
        let hex = s.strip_prefix('#')?;
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        match hex.len() {
            6 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, 0xff)),
            8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Convert to an `iced::Color` (linear 0.0–1.0 components with alpha).
    pub fn iced(self) -> iced::Color {
        iced::Color::from_rgba8(self.r, self.g, self.b, self.a as f32 / 255.0)
    }
}

/// The resolved theme the UI renders against. Every field has a built-in default.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Full-bleed background image; `None` (or a missing file) → solid `background`.
    pub wallpaper: Option<PathBuf>,
    /// Solid fill shown behind/instead of the wallpaper.
    pub background: Color,
    /// The frosted login card fill — translucent (low alpha) over the wallpaper.
    pub card: Color,
    /// Primary accent (focus, the sign-in button).
    pub accent: Color,
    /// Main text/field color.
    pub foreground: Color,
    /// Secondary text (status line, clock).
    pub muted: Color,
    /// Optional logo image shown in the card.
    pub logo: Option<PathBuf>,
    /// Card corner radius (px).
    pub corner_radius: f32,
    /// Card width (px).
    pub card_width: f32,
    /// Whether to show the clock in the card.
    pub show_clock: bool,
}

impl Default for Theme {
    /// The built-in beautiful default: a dark, Tokyo-Night-ish palette with a
    /// translucent card and a soft blue accent. Renders fully even with no assets.
    fn default() -> Self {
        Theme {
            wallpaper: None,
            background: Color::rgb(0x1a, 0x1b, 0x26),
            card: Color::rgba(0x24, 0x28, 0x3b, 0xd0),
            accent: Color::rgb(0x7a, 0xa2, 0xf7),
            foreground: Color::rgb(0xc0, 0xca, 0xf5),
            muted: Color::rgb(0x82, 0x8b, 0xb8),
            logo: None,
            corner_radius: 14.0,
            card_width: 380.0,
            show_clock: true,
        }
    }
}

/// The on-disk form: every field optional, merged over [`Theme::default`]. Strict
/// (`deny_unknown_fields`) so a typo'd key is a loud parse error, not silent stock.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    wallpaper: Option<String>,
    background: Option<String>,
    card: Option<String>,
    accent: Option<String>,
    foreground: Option<String>,
    muted: Option<String>,
    logo: Option<String>,
    corner_radius: Option<f32>,
    card_width: Option<f32>,
    show_clock: Option<bool>,
}

impl Theme {
    /// Resolve the theme from the first readable config path, merged over the
    /// default. Never fails — a missing/invalid file leaves the default standing
    /// (with a stderr note), because the greeter must always render.
    pub fn load() -> Theme {
        match Self::config_source() {
            Some((path, contents)) => match toml::from_str::<ThemeFile>(&contents) {
                Ok(file) => Theme::default().merged(file),
                Err(e) => {
                    eprintln!("door-greeter: ignoring malformed theme {}: {e}", path.display());
                    Theme::default()
                }
            },
            None => Theme::default(),
        }
    }

    /// First readable config (env override, then the system paths) as (path, text).
    fn config_source() -> Option<(PathBuf, String)> {
        let candidates = std::env::var_os(ENV_CONFIG)
            .map(PathBuf::from)
            .into_iter()
            .chain(SYSTEM_CONFIG_PATHS.iter().map(PathBuf::from));
        for path in candidates {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                return Some((path, contents));
            }
        }
        None
    }

    /// Overlay a parsed file onto this theme. A color that fails to parse keeps the
    /// existing value (with a warning) rather than discarding the whole config.
    fn merged(mut self, file: ThemeFile) -> Theme {
        let color = |field: &str, raw: Option<String>, current: Color| -> Color {
            match raw {
                Some(s) => Color::parse(&s).unwrap_or_else(|| {
                    eprintln!("door-greeter: theme '{field}' is not #rrggbb[aa]: {s:?}; keeping default");
                    current
                }),
                None => current,
            }
        };
        self.background = color("background", file.background, self.background);
        self.card = color("card", file.card, self.card);
        self.accent = color("accent", file.accent, self.accent);
        self.foreground = color("foreground", file.foreground, self.foreground);
        self.muted = color("muted", file.muted, self.muted);
        if let Some(w) = file.wallpaper {
            self.wallpaper = Some(PathBuf::from(w));
        }
        if let Some(l) = file.logo {
            self.logo = Some(PathBuf::from(l));
        }
        if let Some(r) = file.corner_radius {
            self.corner_radius = r;
        }
        if let Some(w) = file.card_width {
            self.card_width = w;
        }
        if let Some(c) = file.show_clock {
            self.show_clock = c;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_six_and_eight_digit_hex() {
        assert_eq!(Color::parse("#7aa2f7"), Some(Color::rgba(0x7a, 0xa2, 0xf7, 0xff)));
        assert_eq!(Color::parse("#24283bd0"), Some(Color::rgba(0x24, 0x28, 0x3b, 0xd0)));
        assert_eq!(Color::parse("#FFFFFF"), Some(Color::rgb(255, 255, 255)));
    }

    #[test]
    fn rejects_malformed_hex() {
        for bad in ["7aa2f7", "#abc", "#xyzxyz", "#1234567", ""] {
            assert_eq!(Color::parse(bad), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn empty_file_is_the_default() {
        let merged = Theme::default().merged(ThemeFile::default());
        assert_eq!(merged, Theme::default());
    }

    #[test]
    fn partial_file_overlays_only_named_fields() {
        let file: ThemeFile = toml::from_str(
            r##"
            accent = "#ff0000"
            card_width = 420.0
            show_clock = false
            wallpaper = "/usr/share/door/bg.png"
            "##,
        )
        .unwrap();
        let merged = Theme::default().merged(file);
        assert_eq!(merged.accent, Color::rgb(0xff, 0, 0));
        assert_eq!(merged.card_width, 420.0);
        assert!(!merged.show_clock);
        assert_eq!(merged.wallpaper, Some(PathBuf::from("/usr/share/door/bg.png")));
        // Untouched fields keep the default.
        assert_eq!(merged.background, Theme::default().background);
        assert_eq!(merged.foreground, Theme::default().foreground);
    }

    #[test]
    fn a_bad_color_keeps_the_default_for_that_field() {
        let file: ThemeFile = toml::from_str(r#"accent = "not-a-color""#).unwrap();
        let merged = Theme::default().merged(file);
        assert_eq!(merged.accent, Theme::default().accent);
    }

    #[test]
    fn shipped_default_config_parses_and_is_complete() {
        // Guards the packaged default against drift: it must parse (deny_unknown_fields
        // catches a typo'd key) and produce a usable theme.
        let raw = include_str!("../../dist/door/greeter.toml");
        let file: ThemeFile = toml::from_str(raw).expect("shipped greeter.toml must parse");
        let theme = Theme::default().merged(file);
        assert!(theme.wallpaper.is_some(), "default ships a wallpaper");
        assert!(theme.show_clock);
        assert_eq!(theme.accent, Color::rgb(0x7a, 0xa2, 0xf7));
    }

    #[test]
    fn unknown_keys_are_a_parse_error() {
        // Strict parsing: a typo'd key must fail loudly, not be silently ignored.
        let result = toml::from_str::<ThemeFile>(r##"acent = "#ff0000""##);
        assert!(result.is_err());
    }
}
