//! The animated sky — a Tokyo Night starfield + a drifting comet — drawn over the
//! wallpaper. A native port of the user's Plasma wallpaper plugin
//! (`com.genny.tokyonightcomet`): three parallax star layers (depth-colored, gently
//! twinkling and drifting) and a comet that sweeps upper-right → lower-left on a
//! ~7 s ease, pauses, and loops. Shared by `door-greeter` (its live background) and
//! `door-settings` (the in-window preview), so both animate identically.
//!
//! It is a `canvas::Program` generic over the host's `Message` (it emits none), so
//! either app can drop it into a `stack` behind its UI.

use iced::widget::canvas::{gradient, Frame, Geometry, Path, Program};
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

/// Linear blend of two colors (ignoring alpha — callers set that via `with_alpha`).
fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
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
    /// The rotating comet's color (head + trail).
    pub comet: Color,
    /// The static ring of dots.
    pub track: Color,
    /// Trail length 0.2–1.0 (shorter = crisper; longer = a softer ribbon).
    pub trail: f32,
    /// Head-glow intensity (0 = crisp/no bloom).
    pub glow: f32,
    /// Rotation speed (rad/s).
    pub speed: f32,
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
        use std::f32::consts::{PI, TAU};
        let mut frame = Frame::new(renderer, bounds.size());
        let size = bounds.width.min(bounds.height);
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let ring = size * 0.36;
        let dot = size * 0.075;
        let at = |angle: f32| Point::new(center.x + angle.cos() * ring, center.y + angle.sin() * ring);

        // `hot` ties the white-hot/bloom look to `glow`: on the dark night card a
        // glowing comet over a bright white-blue nucleus reads beautifully; the day
        // card defaults `glow` to 0, which keeps the head a crisp pure-comet dot with
        // no white wash and no banding on the light background.
        let hot = self.glow.clamp(0.0, 1.0);
        let white = rgb(0xff, 0xff, 0xff);
        // Head brightness gently breathes so the spinner feels alive even at rest.
        let pulse = 0.86 + 0.14 * (self.anim * 3.0).sin();

        // A faint static orbit the comet rides — a hairline ring plus sparse dots, so
        // the path reads even when the comet is on the far side.
        frame.stroke(
            &Path::circle(center, ring),
            iced::widget::canvas::Stroke {
                style: with_alpha(self.track, 0.10 * self.fade).into(),
                width: (size * 0.012).max(0.6),
                ..Default::default()
            },
        );
        const TRACK: usize = 12;
        for i in 0..TRACK {
            let a = i as f32 / TRACK as f32 * TAU;
            frame.fill(&Path::circle(at(a), dot * 0.4), with_alpha(self.track, 0.16 * self.fade));
        }

        // The comet trail: a dense ribbon of dots from tail → head, each blended from
        // pure comet (tail) toward a white-blue hot core (head, scaled by `hot`), with
        // a smooth size + alpha taper so it reads as a luminous streak rather than
        // beads. Density scales with `trail`. Drawn tail-first so the head sits on top.
        let head = self.anim * self.speed;
        let trail = self.trail.clamp(0.15, 1.0);
        let arc = 2.6 * trail;
        let dots = ((72.0 * trail).round() as usize).max(12);
        for j in (0..dots).rev() {
            let k = j as f32 / dots as f32; // 0 head .. 1 tail
            let a = head - k * arc;
            let r = dot * (1.0 - 0.55 * k);
            let alpha = (1.0 - k).powf(1.7) * self.fade;
            let col = mix(self.comet, white, hot * (1.0 - k).powi(2) * 0.85);
            frame.fill(&Path::circle(at(a), r.max(0.6)), with_alpha(col, alpha));
        }

        let h = at(head);
        // Soft coma halo around the head — layered low-alpha discs (night only; bands
        // on the light day card, where `glow` is 0).
        if self.glow > 0.0 {
            for &(rr, oo) in &[(3.4f32, 0.05f32), (2.5, 0.09), (1.7, 0.16), (1.1, 0.28)] {
                let c = mix(self.comet, white, hot * 0.5);
                frame.fill(&Path::circle(h, dot * rr), with_alpha(c, oo * self.glow * pulse * self.fade));
            }
        }

        // The bright nucleus — a hot near-white core (night) / crisp comet dot (day),
        // with a tiny white center pip for sparkle.
        let core = mix(self.comet, white, hot * 0.8);
        frame.fill(&Path::circle(h, dot * 0.78), with_alpha(core, pulse * self.fade));
        frame.fill(&Path::circle(h, dot * 0.32), with_alpha(white, (0.35 + 0.55 * hot) * pulse * self.fade));

        // A 4-point star glint over the head — the classic comet sparkle. Spike reach
        // and brightness grow with `glow`; on the day card it's a small crisp cross.
        let reach = dot * (1.6 + 3.2 * hot);
        let glint = Path::new(|b| {
            for i in 0..8 {
                let ang = i as f32 / 8.0 * TAU;
                let rad = if i % 2 == 0 { reach } else { dot * 0.28 };
                let p = Point::new(h.x + ang.cos() * rad, h.y + ang.sin() * rad);
                if i == 0 { b.move_to(p); } else { b.line_to(p); }
            }
            b.close();
        });
        frame.fill(&glint, with_alpha(mix(self.comet, white, 0.4 + 0.5 * hot), (0.30 + 0.45 * hot) * pulse * self.fade));

        // Shed sparkles — a few tiny twinkling motes riding just off the trail, each on
        // its own phase, for a touch of magic. Subtle and color-matched so they read on
        // either card.
        for s in 0..3 {
            let off = s as f32 * 2.3 + 0.7;
            let ka = head - (0.4 + 0.5 * s as f32) * arc;
            let wob = (self.anim * (1.7 + 0.4 * s as f32) + off).sin();
            let twk = 0.5 + 0.5 * (self.anim * (3.1 + s as f32) + off * PI).sin();
            let rr = ring + wob * dot * 1.4;
            let p = Point::new(center.x + ka.cos() * rr, center.y + ka.sin() * rr);
            frame.fill(&Path::circle(p, dot * 0.22), with_alpha(self.comet, 0.5 * twk * self.fade));
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

        // Day gets atmosphere layers so a light background reads with the same depth
        // as night instead of a flat white field of stars: a soft vertical sky wash
        // (deeper periwinkle up top, clearing toward the bottom) under everything.
        if self.day {
            let grad = gradient::Linear::new(Point::new(w * 0.5, 0.0), Point::new(w * 0.5, h))
                .add_stop(0.0, with_alpha(rgb(0xa8, 0xbd, 0xee), 0.60 * self.fade))
                .add_stop(0.5, with_alpha(rgb(0xc8, 0xd5, 0xf2), 0.30 * self.fade))
                .add_stop(1.0, with_alpha(rgb(0xe9, 0xec, 0xf4), 0.0));
            frame.fill(&Path::rectangle(Point::new(0.0, 0.0), bounds.size()), grad);
        }

        // Soft depth-glow (matches the desktop comet plugin), so the background has
        // depth rather than reading flat. Stacked translucent circles approximate a
        // radial since canvas fills are flat. Day uses a larger, slightly stronger
        // sky-blue haze (a soft sun) to stay luminous against the light wash.
        let glow_center = Point::new(w * 0.5, h * 0.42);
        let glow_r = w.min(h) * if self.day { 0.78 } else { 0.6 };
        let glow_col = if self.day { rgb(0x8f, 0xb6, 0xff) } else { indigo };
        let glow_alpha = if self.day { 0.020 } else { 0.012 };
        for i in 0..16 {
            let _t = i as f32 / 15.0; // 0 = widest/faintest .. 1 = innermost
            let radius = glow_r * (1.0 - 0.62 * _t);
            frame.fill(
                &Path::circle(glow_center, radius),
                with_alpha(glow_col, glow_alpha * self.fade),
            );
        }

        // Starfield: parallax drift + twinkle, colored by depth. Night shows the full
        // field; day keeps only the sparse brightest tier as faint daytime sparkles
        // (a white background made every star glaringly visible and flat).
        let star_mul = if self.day { 0.45 } else { 0.9 };
        for s in &self.stars {
            if self.day && s.tier < 2 {
                continue;
            }
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
