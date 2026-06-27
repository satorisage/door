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
const INDIGO: Color = rgb(0x3d, 0x59, 0xa1);

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
/// `fade` (0–1) ramps the whole thing in on launch; `day` recolors for the light
/// (Tokyo Night Day) variant.
pub struct Sky {
    pub stars: Vec<Star>,
    pub anim: f32,
    pub fade: f32,
    pub day: bool,
}

/// (core, bright/blue, brightest/cyan, glow/indigo) for the night or day variant.
/// Day uses vivid blues/teal that read on a light background.
fn palette(day: bool) -> (Color, Color, Color, Color) {
    if day {
        // Harmonious blues that read on light without the harsh teal; a soft,
        // airy glow (Bob-Ross cloud, not a hard disc).
        (
            rgb(0x3a, 0x5f, 0xb0),
            rgb(0x2e, 0x7d, 0xe9),
            rgb(0x5a, 0xa0, 0xf0),
            rgb(0x9c, 0xc0, 0xff),
        )
    } else {
        (CORE, BLUE, CYAN, INDIGO)
    }
}

fn tier_color(tier: u8, core: Color, blue: Color, cyan: Color) -> Color {
    match tier {
        2 => cyan,
        1 => blue,
        _ => core,
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
    pub day: bool,
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
        let at = |angle: f32| Point::new(center.x + angle.cos() * ring, center.y + angle.sin() * ring);
        let (core, blue, cyan, _) = palette(self.day);
        // Head→mid→tail colors. Night glows bright-on-dark; day must invert the
        // value (a deep comet inking onto a light card) or it washes out to nothing.
        let (c_head, c_mid, c_tail, bloom_col) = if self.day {
            (
                rgb(0x21, 0x46, 0x93),
                rgb(0x2e, 0x7d, 0xe9),
                rgb(0x52, 0x82, 0xd8),
                rgb(0x2e, 0x7d, 0xe9),
            )
        } else {
            (core, cyan, blue, cyan)
        };
        let track_alpha = if self.day { 0.16 } else { 0.12 };
        let bloom_mul = if self.day { 0.30 } else { 1.0 };

        // A faint static track of dots.
        const TRACK: usize = 12;
        for i in 0..TRACK {
            let a = i as f32 / TRACK as f32 * std::f32::consts::TAU;
            frame.fill(&Path::circle(at(a), dot * 0.5), with_alpha(c_tail, track_alpha * self.fade));
        }

        // The comet: a bright head + fading trail at a *continuous* angle, so it
        // glides smoothly around the ring rather than snapping between track dots.
        let head = self.anim * 2.5; // ~0.4 rev/s
        // A dense, overlapping trail reads as one continuous silk ribbon rather
        // than separate dots; a smooth taper in radius and alpha toward the tail.
        const TRAIL: usize = 40;
        for j in 0..TRAIL {
            let k = j as f32 / TRAIL as f32; // 0 head .. ~1 tail
            let a = head - k * 2.6; // trail sweeps ~2.6 rad behind the head
            let r = dot * (1.0 - 0.5 * k);
            let col = if j == 0 {
                c_head
            } else if k < 0.4 {
                c_mid
            } else {
                c_tail
            };
            let alpha = (1.0 - k).powf(1.6) * self.fade;
            frame.fill(&Path::circle(at(a), r.max(0.6)), with_alpha(col, alpha));
        }
        // Soft layered glow on the head for a silky bloom (gentler in day).
        for &(rr, oo) in &[(2.2f32, 0.10f32), (1.6, 0.16), (1.05, 0.30)] {
            frame.fill(&Path::circle(at(head), dot * rr), with_alpha(bloom_col, oo * bloom_mul * self.fade));
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
        let (core, blue, cyan, indigo) = palette(self.day);
        // Day stars sit much dimmer so they stay subtle on the light background.
        let star_mul = if self.day { 0.5 } else { 0.9 };

        // Soft depth-glow (matches the desktop comet plugin), so the solid
        // background has depth rather than reading flat. Stacked translucent circles
        // approximate a radial since canvas fills are flat.
        let glow_center = Point::new(w * 0.5, h * 0.42);
        let glow_r = w.min(h) * 0.6;
        for i in 0..16 {
            let _t = i as f32 / 15.0; // 0 = widest/faintest .. 1 = innermost
            let radius = glow_r * (1.0 - 0.62 * _t);
            frame.fill(
                &Path::circle(glow_center, radius),
                with_alpha(indigo, 0.012 * self.fade),
            );
        }

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
            let col = with_alpha(tier_color(s.tier, core, blue, cyan), bright * self.fade * star_mul);
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
                let col = if t < 0.5 { blue } else { cyan };
                frame.fill(
                    &Path::circle(Point::new(hx + tx * off, hy + ty * off), sz.max(0.6)),
                    with_alpha(col, alpha),
                );
            }
            // Head glow (stacked translucent circles) + a bright core.
            for &(r, o) in &[(34.0f32, 0.16f32), (20.0, 0.28), (11.0, 0.5)] {
                frame.fill(&Path::circle(Point::new(hx, hy), r), with_alpha(cyan, o * self.fade));
            }
            frame.fill(&Path::circle(Point::new(hx, hy), 5.0), with_alpha(core, self.fade));
        }

        vec![frame.into_geometry()]
    }
}
