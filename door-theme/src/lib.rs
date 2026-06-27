//! The greeter's look (M4): a config-driven theme with a built-in beautiful
//! default. Shared by `door-greeter` (which renders it) and `door-settings` (which
//! edits it) — one source of truth for the schema and the colors.
//!
//! The greeter is the pre-auth surface, so the config is admin-controlled and
//! world-readable — never user-supplied at login time. It is loaded from the first
//! file that exists, each merged *over* the built-in [`Theme::default`] so a partial
//! file (just an `accent`, say) works:
//!
//! 1. `$DOORD_GREETER_CONFIG` — explicit path, for dev/testing
//! 2. `/etc/door/greeter.toml` — admin override
//! 3. `/usr/share/door/greeter.toml` — packaged default
//!
//! If none exist or one fails to parse, the built-in default stands (the greeter
//! must always render). Asset paths (wallpaper, font, logo) point at world-readable
//! files installed system-wide; a missing asset degrades to the solid background /
//! stock font rather than failing.

pub mod sky;

use serde::Deserialize;
use std::path::PathBuf;

/// Explicit config path override (dev/testing). Checked before the system paths.
pub const ENV_CONFIG: &str = "DOORD_GREETER_CONFIG";
/// The admin override path — where `door-settings` saves.
pub const ETC_CONFIG: &str = "/etc/door/greeter.toml";
/// Admin override, then the packaged default. First readable wins.
const SYSTEM_CONFIG_PATHS: [&str; 2] = [ETC_CONFIG, "/usr/share/door/greeter.toml"];

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
    pub fn parse(s: &str) -> Option<Color> {
        let hex = s.strip_prefix('#')?;
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        match hex.len() {
            6 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, 0xff)),
            8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Render as `#rrggbb` (opaque) or `#rrggbbaa` (with alpha) — the inverse of
    /// [`parse`](Self::parse), used when writing a config.
    pub fn to_hex(self) -> String {
        if self.a == 0xff {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Convert to an `iced::Color` (linear 0.0–1.0 components with alpha).
    pub fn iced(self) -> iced::Color {
        iced::Color::from_rgba8(self.r, self.g, self.b, self.a as f32 / 255.0)
    }

    /// As an `iced::Color` with the alpha scaled by `mult` (0.0–1.0) — drives the
    /// launch fade-in (the card and its text ramp up together).
    pub fn iced_alpha(self, mult: f32) -> iced::Color {
        iced::Color::from_rgba8(self.r, self.g, self.b, (self.a as f32 / 255.0) * mult)
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
    /// Secondary text (status line, date, placeholders).
    pub muted: Color,
    /// Input field fill (slightly lifted from the card).
    pub field: Color,
    /// Optional logo image shown in the card.
    pub logo: Option<PathBuf>,
    /// Optional font family name (must be installed system-wide); `None` → stock.
    pub font: Option<String>,
    /// Card corner radius (px).
    pub corner_radius: f32,
    /// Card width (px).
    pub card_width: f32,
    /// Whether to show the clock + date in the card.
    pub show_clock: bool,
    /// Whether to run the animated sky (twinkling stars + drifting comet). Off → a
    /// still wallpaper, like the battery half of the Plasma comet wallpaper.
    pub animate: bool,
    /// True for the light (Tokyo Night Day) variant — the sky recolors for a light
    /// background. Set by which built-in this resolved from, not from the file.
    pub is_day: bool,
}

impl Default for Theme {
    /// The built-in beautiful default: a dark, Tokyo-Night palette with a
    /// translucent card and a soft blue accent. Renders fully even with no assets.
    fn default() -> Self {
        Theme {
            wallpaper: None,
            background: Color::rgb(0x1a, 0x1b, 0x26),
            // A dark, glassy card — translucent so the wallpaper reads through.
            card: Color::rgba(0x16, 0x16, 0x1e, 0xd8),
            accent: Color::rgb(0x7a, 0xa2, 0xf7),
            foreground: Color::rgb(0xc0, 0xca, 0xf5),
            muted: Color::rgb(0x56, 0x5f, 0x89),
            field: Color::rgb(0x29, 0x2e, 0x42),
            logo: None,
            font: None,
            corner_radius: 16.0,
            card_width: 300.0,
            show_clock: true,
            animate: true,
            is_day: false,
        }
    }
}

impl Theme {
    /// The built-in **day** (Tokyo Night Day, light) palette — the light twin of
    /// [`Theme::default`]. Structural keys (font, sizes, behavior) are taken from
    /// the config at load time; colors and `is_day` come from here.
    pub fn day() -> Self {
        Theme {
            wallpaper: None,
            background: Color::rgb(0xe1, 0xe2, 0xe7),
            // Frosted white glass — translucent so the day sky reads through it.
            card: Color::rgba(0xf4, 0xf6, 0xfb, 0xa6),
            accent: Color::rgb(0x2e, 0x7d, 0xe9),
            foreground: Color::rgb(0x34, 0x3b, 0x58),
            muted: Color::rgb(0x6a, 0x73, 0x9e),
            field: Color::rgba(0xff, 0xff, 0xff, 0x99),
            logo: None,
            font: None,
            corner_radius: 16.0,
            card_width: 300.0,
            show_clock: true,
            animate: true,
            is_day: true,
        }
    }
}

/// The on-disk form: every field optional, merged over [`Theme::default`]. Strict
/// (`deny_unknown_fields`) so a typo'd key is a loud parse error, not silent stock.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    wallpaper: Option<String>,
    background: Option<String>,
    card: Option<String>,
    accent: Option<String>,
    foreground: Option<String>,
    muted: Option<String>,
    field: Option<String>,
    font: Option<String>,
    logo: Option<String>,
    corner_radius: Option<f32>,
    card_width: Option<f32>,
    show_clock: Option<bool>,
    animate: Option<bool>,
    /// Local day window for the greeter's auto day/night, `"HH:MM"` (default
    /// 07:00–19:00). Inside the window the greeter uses the day palette.
    day_start: Option<String>,
    day_end: Option<String>,
    /// `[day]` table — color/wallpaper/logo overrides for the day variant (over the
    /// built-in Tokyo Night Day palette). Structural keys are shared from top level.
    day: Option<DayFile>,
}

/// The `[day]` override table: the visual (per-variant) keys only.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct DayFile {
    wallpaper: Option<String>,
    background: Option<String>,
    card: Option<String>,
    accent: Option<String>,
    foreground: Option<String>,
    muted: Option<String>,
    field: Option<String>,
    logo: Option<String>,
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
                    eprintln!("door: ignoring malformed theme {}: {e}", path.display());
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
                    eprintln!("door: theme '{field}' is not #rrggbb[aa]: {s:?}; keeping default");
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
        self.field = color("field", file.field, self.field);
        if let Some(w) = file.wallpaper {
            self.wallpaper = Some(PathBuf::from(w));
        }
        if let Some(l) = file.logo {
            self.logo = Some(PathBuf::from(l));
        }
        if file.font.is_some() {
            self.font = file.font;
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
        if let Some(a) = file.animate {
            self.animate = a;
        }
        self
    }

    /// Apply only the *structural* keys (font, sizes, behavior) from a file —
    /// shared across day/night. Colors and wallpaper are left to the variant.
    fn merged_structural(mut self, file: &ThemeFile) -> Theme {
        if file.font.is_some() {
            self.font = file.font.clone();
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
        if let Some(a) = file.animate {
            self.animate = a;
        }
        self
    }

    /// Resolve the theme for a given local time (`now_minutes` = hour*60+min),
    /// auto-selecting the day or night variant by the configured window. The
    /// greeter calls this; it cannot read the user's color scheme (it runs before
    /// login), so the clock is the trigger. Night = the config's top-level palette;
    /// day = the built-in light palette with the config's structural keys.
    pub fn load_at(now_minutes: u32) -> Theme {
        let (file, have) = match Self::config_source() {
            Some((path, contents)) => match toml::from_str::<ThemeFile>(&contents) {
                Ok(file) => (file, true),
                Err(e) => {
                    eprintln!("door: ignoring malformed theme {}: {e}", path.display());
                    (ThemeFile::default(), false)
                }
            },
            None => (ThemeFile::default(), false),
        };
        let (start, end) = day_window(&file);
        if in_window(now_minutes, start, end) {
            Theme::day().merged_structural(&file).merged_day(file.day)
        } else if have {
            Theme::default().merged(file)
        } else {
            Theme::default()
        }
    }

    /// Apply the `[day]` color/wallpaper/logo overrides over the built-in day palette.
    fn merged_day(mut self, day: Option<DayFile>) -> Theme {
        let Some(d) = day else {
            return self;
        };
        let color = |field: &str, raw: Option<String>, current: Color| match raw {
            Some(s) => Color::parse(&s).unwrap_or_else(|| {
                eprintln!("door: [day].{field} is not #rrggbb[aa]: {s:?}; keeping default");
                current
            }),
            None => current,
        };
        self.background = color("background", d.background, self.background);
        self.card = color("card", d.card, self.card);
        self.accent = color("accent", d.accent, self.accent);
        self.foreground = color("foreground", d.foreground, self.foreground);
        self.muted = color("muted", d.muted, self.muted);
        self.field = color("field", d.field, self.field);
        if let Some(w) = d.wallpaper {
            self.wallpaper = Some(PathBuf::from(w));
        }
        if let Some(l) = d.logo {
            self.logo = Some(PathBuf::from(l));
        }
        self
    }

    /// Load both variants + the day window — for `door-settings`, which edits both.
    pub fn load_pair() -> (Theme, Theme, (u32, u32)) {
        let file = match Self::config_source() {
            Some((path, contents)) => toml::from_str::<ThemeFile>(&contents).unwrap_or_else(|e| {
                eprintln!("door: ignoring malformed theme {}: {e}", path.display());
                ThemeFile::default()
            }),
            None => ThemeFile::default(),
        };
        let window = day_window(&file);
        let night = Theme::default().merged(file.clone());
        let day = Theme::day().merged_structural(&file).merged_day(file.day.clone());
        (night, day, window)
    }

    /// Render a full `greeter.toml` with both palettes — the night top-level, the
    /// day window, and a `[day]` table. What `door-settings` saves.
    pub fn render_pair(night: &Theme, day: &Theme, day_start: &str, day_end: &str) -> String {
        let mut out = night.to_config_string();
        out.push_str(&format!("day_start  = {day_start:?}\n"));
        out.push_str(&format!("day_end    = {day_end:?}\n"));
        out.push_str("\n# Day variant overrides (light). Structural keys (font, sizes,\n");
        out.push_str("# behaviour) are shared from above; only colors/assets differ.\n");
        out.push_str("[day]\n");
        out.push_str(&format!("background  = {:?}\n", day.background.to_hex()));
        out.push_str(&format!("card        = {:?}\n", day.card.to_hex()));
        out.push_str(&format!("field       = {:?}\n", day.field.to_hex()));
        out.push_str(&format!("accent      = {:?}\n", day.accent.to_hex()));
        out.push_str(&format!("foreground  = {:?}\n", day.foreground.to_hex()));
        out.push_str(&format!("muted       = {:?}\n", day.muted.to_hex()));
        match &day.wallpaper {
            Some(w) => out.push_str(&format!("wallpaper = {:?}\n", w.display().to_string())),
            None => out.push_str("# wallpaper =\n"),
        }
        match &day.logo {
            Some(l) => out.push_str(&format!("logo = {:?}\n", l.display().to_string())),
            None => out.push_str("# logo =\n"),
        }
        out
    }

    /// Render this theme as a documented `greeter.toml` — what `door-settings`
    /// writes. Mirrors the packaged default's layout so a saved file stays readable
    /// and re-editable by hand.
    pub fn to_config_string(&self) -> String {
        let mut out = String::new();
        out.push_str("# door greeter theme — written by door-settings. Edit here or in\n");
        out.push_str("# door-settings; keys are documented in the packaged default at\n");
        out.push_str("# /usr/share/door/greeter.toml. Colors are \"#rrggbb\" or \"#rrggbbaa\".\n\n");
        match &self.wallpaper {
            Some(w) => out.push_str(&format!("wallpaper = {:?}\n", w.display().to_string())),
            None => out.push_str("# wallpaper =   # (none — solid background)\n"),
        }
        out.push_str(&format!("background  = {:?}\n", self.background.to_hex()));
        out.push_str(&format!("card        = {:?}\n", self.card.to_hex()));
        out.push_str(&format!("field       = {:?}\n", self.field.to_hex()));
        out.push_str(&format!("accent      = {:?}\n", self.accent.to_hex()));
        out.push_str(&format!("foreground  = {:?}\n", self.foreground.to_hex()));
        out.push_str(&format!("muted       = {:?}\n", self.muted.to_hex()));
        match &self.font {
            Some(f) => out.push_str(&format!("font = {f:?}\n")),
            None => out.push_str("# font =   # (stock)\n"),
        }
        match &self.logo {
            Some(l) => out.push_str(&format!("logo = {:?}\n", l.display().to_string())),
            None => out.push_str("# logo =\n"),
        }
        out.push_str(&format!("corner_radius = {}\n", self.corner_radius));
        out.push_str(&format!("card_width    = {}\n", self.card_width));
        out.push_str(&format!("show_clock    = {}\n", self.show_clock));
        out.push_str(&format!("animate       = {}\n", self.animate));
        out
    }
}

/// Parse `"HH:MM"` to minutes-since-midnight; `None` if malformed.
fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// The configured day window in minutes (default 07:00–19:00).
fn day_window(file: &ThemeFile) -> (u32, u32) {
    let start = file.day_start.as_deref().and_then(parse_hhmm).unwrap_or(7 * 60);
    let end = file.day_end.as_deref().and_then(parse_hhmm).unwrap_or(19 * 60);
    (start, end)
}

/// Is `now` within `[start, end)`? Handles a window that wraps past midnight.
fn in_window(now: u32, start: u32, end: u32) -> bool {
    if start <= end {
        now >= start && now < end
    } else {
        now >= start || now < end
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
    fn hex_round_trips() {
        for s in ["#7aa2f7", "#24283bd0", "#16161e"] {
            assert_eq!(Color::parse(s).unwrap().to_hex(), s);
        }
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
    fn written_config_round_trips_through_parsing() {
        // What door-settings writes must parse back to the same theme.
        let mut t = Theme::default();
        t.accent = Color::rgb(0xbb, 0x9a, 0xf7);
        t.wallpaper = Some(PathBuf::from("/usr/share/door/wallpaper.png"));
        t.font = Some("MesloLGS Nerd Font".to_string());
        t.show_clock = false;
        let rendered = t.to_config_string();
        let file: ThemeFile = toml::from_str(&rendered).expect("written config must parse");
        assert_eq!(Theme::default().merged(file), t);
    }

    #[test]
    fn shipped_default_config_parses_and_is_complete() {
        let raw = include_str!("../../dist/door/greeter.toml");
        let file: ThemeFile = toml::from_str(raw).expect("shipped greeter.toml must parse");
        let theme = Theme::default().merged(file);
        // Default is the clean animated sky on a solid bg (no static wallpaper).
        assert!(theme.wallpaper.is_none(), "default uses the animated sky, no image");
        assert!(theme.animate);
        assert!(theme.show_clock);
        assert_eq!(theme.accent, Color::rgb(0x7a, 0xa2, 0xf7));
    }

    #[test]
    fn day_window_parsing_and_membership() {
        assert_eq!(parse_hhmm("07:00"), Some(420));
        assert_eq!(parse_hhmm("19:30"), Some(1170));
        assert_eq!(parse_hhmm("nope"), None);
        assert_eq!(parse_hhmm("24:00"), None);
        // default 07:00–19:00
        assert!(in_window(12 * 60, 420, 1140)); // noon = day
        assert!(!in_window(6 * 60, 420, 1140)); // 06:00 = night
        assert!(!in_window(20 * 60, 420, 1140)); // 20:00 = night
        // a window that wraps midnight
        assert!(in_window(23 * 60, 22 * 60, 5 * 60));
        assert!(in_window(2 * 60, 22 * 60, 5 * 60));
        assert!(!in_window(12 * 60, 22 * 60, 5 * 60));
    }

    #[test]
    fn render_pair_round_trips_both_palettes_and_window() {
        let mut night = Theme::default();
        night.accent = Color::rgb(0xbb, 0x9a, 0xf7);
        let mut day = Theme::day();
        day.accent = Color::rgb(0x12, 0x34, 0x56);
        day.background = Color::rgb(0xff, 0xff, 0xff);
        let s = Theme::render_pair(&night, &day, "06:30", "18:45");
        let file: ThemeFile = toml::from_str(&s).expect("render_pair must parse");
        let n2 = Theme::default().merged(file.clone());
        let d2 = Theme::day()
            .merged_structural(&file)
            .merged_day(file.day.clone());
        assert_eq!(n2.accent, night.accent);
        assert_eq!(d2.accent, day.accent);
        assert_eq!(d2.background, day.background);
        assert_eq!(day_window(&file), (6 * 60 + 30, 18 * 60 + 45));
    }

    #[test]
    fn day_variant_is_light_and_flagged() {
        let day = Theme::day();
        assert!(day.is_day);
        assert!(!Theme::default().is_day);
        // Day background is light; night is dark.
        assert!(day.background.r > Theme::default().background.r);
    }

    #[test]
    fn unknown_keys_are_a_parse_error() {
        let result = toml::from_str::<ThemeFile>(r##"acent = "#ff0000""##);
        assert!(result.is_err());
    }
}
