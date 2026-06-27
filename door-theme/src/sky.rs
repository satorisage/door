//! The animated sky — a Tokyo Night starfield + a drifting comet — drawn over the
//! wallpaper. A native port of the user's Plasma wallpaper plugin
//! (`com.genny.tokyonightcomet`): three parallax star layers (depth-colored, gently
//! twinkling and drifting) and a comet that sweeps upper-right → lower-left on a
//! ~7 s ease, pauses, and loops. Shared by `door-greeter` (its live background) and
//! `door-settings` (the in-window preview), so both animate identically.
//!
//! It is a `canvas::Program` generic over the host's `Message` (it emits none), so
//! either app can drop it into a `stack` behind its UI.

use iced::widget::canvas::{Frame, Geometry, Path, Program};
use iced::{mouse, Color, Point, Rectangle, Renderer};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}
const CORE: Color = rgb(0xc0, 0xca, 0xf5);
const BLUE: Color = rgb(0x7a, 0xa2, 0xf7);
const CYAN: Color = rgb(0x7d, 0xcf, 0xff);

/// Comet timing: a 7 s sweep then a 2.5 s pause, looping (matches the wallpaper).
const SWEEP: f32 = 7.0;
const PERIOD: f32 = 9.5;

/// One star: position as a fraction of the screen, radius (px), a twinkle phase,
/// a horizontal parallax-drift amplitude (px), whether it twinkles, and a depth
/// color tier (0 core, 1 blue, 2 cyan).
#[derive(Clone, Copy)]
pub struct Star {
    x: f32,
    y: f32,
    r: f32,
    phase: f32,
    drift: f32,
    twinkles: bool,
    tier: u8,
}

/// The full starfield: three parallax layers (far/mid/near), deterministically
/// placed so they never jump between frames or between the greeter and the preview.
pub fn stars() -> Vec<Star> {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as f32 / (1u64 << 31) as f32 // 0.0..1.0
    };
    // (count, min radius, max radius, drift px, twinkle chance) — the wallpaper's layers.
    let layers = [
        (130usize, 0.5f32, 1.1f32, 0.0f32, 0.22f32),
        (70, 1.0, 1.8, 24.0, 0.40),
        (26, 1.6, 2.8, 60.0, 0.70),
    ];
    let mut out = Vec::new();
    for &(count, min_r, max_r, drift, twinkle) in &layers {
        for _ in 0..count {
            let rs = next();
            let tier = if rs > 0.82 {
                2
            } else if rs > 0.5 {
                1
            } else {
                0
            };
            out.push(Star {
                x: next(),
                y: next(),
                r: min_r + (max_r - min_r) * next(),
                phase: next() * std::f32::consts::TAU,
                drift,
                twinkles: next() < twinkle,
                tier,
            });
        }
    }
    out
}

/// The animated sky canvas. `anim` is the continuously-advancing clock (seconds-ish);
/// `fade` (0–1) ramps the whole thing in on launch.
pub struct Sky {
    pub stars: Vec<Star>,
    pub anim: f32,
    pub fade: f32,
}

fn tier_color(tier: u8) -> Color {
    match tier {
        2 => CYAN,
        1 => BLUE,
        _ => CORE,
    }
}

fn with_alpha(mut c: Color, a: f32) -> Color {
    c.a = a;
    c
}

/// InOutSine ease, matching the wallpaper's comet motion.
fn ease_in_out(x: f32) -> f32 {
    0.5 * (1.0 - (std::f32::consts::PI * x).cos())
}

/// The comet spinner — a rotating ring of dots with a bright comet head trailing
/// off into dimmer dots, a native port of the boot throbber. Used as the card's
/// default logo emblem. `anim` is the shared animation clock; `fade` ramps it in.
pub struct Spinner {
    pub anim: f32,
    pub fade: f32,
}

impl<Message> Program<Message> for Spinner {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = bounds.width.min(bounds.height);
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let ring = size * 0.36;
        let dot = size * 0.075;
        const N: usize = 12;
        // The bright head rotates around the ring; each dot dims with angular
        // distance behind it — a comet head with a trailing tail.
        let head = self.anim * 2.2;
        for i in 0..N {
            let a = i as f32 / N as f32 * std::f32::consts::TAU;
            let behind = (a - head).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
            let bright = (1.0 - behind).powf(1.6);
            let col = if bright > 0.6 {
                CYAN
            } else if bright > 0.3 {
                BLUE
            } else {
                CORE
            };
            let x = center.x + a.cos() * ring;
            let y = center.y + a.sin() * ring;
            frame.fill(
                &Path::circle(Point::new(x, y), dot * (0.55 + 0.45 * bright)),
                with_alpha(col, (0.12 + 0.88 * bright) * self.fade),
            );
        }
        vec![frame.into_geometry()]
    }
}

impl<Message> Program<Message> for Sky {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let (w, h) = (bounds.width, bounds.height);

        // Starfield: parallax drift + twinkle, colored by depth.
        for s in &self.stars {
            let dx = if s.drift != 0.0 {
                (self.anim * 0.25 + s.phase).sin() * s.drift
            } else {
                0.0
            };
            let bright = if s.twinkles {
                0.35 + 0.65 * (0.5 + 0.5 * (self.anim * 1.5 + s.phase).sin())
            } else {
                0.85
            };
            let col = with_alpha(tier_color(s.tier), bright * self.fade);
            frame.fill(&Path::circle(Point::new(s.x * w + dx, s.y * h), s.r), col);
        }

        // Comet: sweep upper-right → lower-left on an InOutSine ease, then pause.
        let u = (self.anim / PERIOD).fract();
        let sweep_frac = SWEEP / PERIOD;
        if u < sweep_frac {
            let p = ease_in_out(u / sweep_frac);
            let hx = (1.08 + (-0.22 - 1.08) * p) * w;
            let hy = (0.10 + (0.65 - 0.10) * p) * h;
            // Travel is down-left; the trail recedes up-right from the head.
            let (tx, ty) = (0.911f32, -0.412f32);
            // 22 trail dots, shrinking + fading toward the tail, indigo→blue→cyan.
            for i in 0..22 {
                let t = i as f32 / 21.0;
                let off = t * (0.16 * w.min(h)); // trail length scales with the screen
                let sz = (2.0 + 13.0 * (1.0 - t)) * 0.5;
                let alpha = (0.06 + 0.7 * (1.0 - t).powf(1.4)) * self.fade;
                let col = if t < 0.5 { BLUE } else { CYAN };
                frame.fill(
                    &Path::circle(Point::new(hx + tx * off, hy + ty * off), sz.max(0.6)),
                    with_alpha(col, alpha),
                );
            }
            // Head glow (stacked translucent circles) + a bright core.
            for &(r, o) in &[(34.0f32, 0.16f32), (20.0, 0.28), (11.0, 0.5)] {
                frame.fill(&Path::circle(Point::new(hx, hy), r), with_alpha(CYAN, o * self.fade));
            }
            frame.fill(&Path::circle(Point::new(hx, hy), 5.0), with_alpha(CORE, self.fade));
        }

        vec![frame.into_geometry()]
    }
}
