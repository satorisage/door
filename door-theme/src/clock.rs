//! The analog clock face — a ticked rim with hour/minute (and optional second)
//! hands, themed from the card palette. A `canvas::Program` generic over the host's
//! `Message` (it emits none), shared by `door-greeter` (the real card clock) and
//! `door-settings` (the in-window preview) so both draw identically.
//!
//! It is a *pure drawing* widget: the caller supplies the hand angles, so the time
//! source stays in each binary (the greeter reads the local clock via `libc`; the
//! settings preview animates a representative pose). Angles are radians, measured
//! clockwise from 12 o'clock.

use iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke};
use iced::{mouse, Color, Point, Rectangle, Renderer};

use crate::Theme;

/// Hand angles in radians, clockwise from 12 o'clock: `(hour, minute, second)`.
pub type Angles = (f32, f32, f32);

/// A themed analog clock face. Build with [`AnalogClock::new`]; drop into a `canvas`
/// widget sized to the desired diameter.
pub struct AnalogClock {
    angles: Angles,
    rim: Color,
    tick: Color,
    tick_minor: Color,
    hand: Color,
    second: Color,
    show_seconds: bool,
}

impl AnalogClock {
    /// Resolve the face colors from the theme palette, faded by `fade` (the launch
    /// fade-in alpha, 0–1). `angles` are the live hand positions. The second hand is
    /// drawn only when the theme shows seconds (which reduced-motion clears).
    pub fn new(t: &Theme, fade: f32, angles: Angles) -> Self {
        Self {
            angles,
            rim: t.muted.iced_alpha(fade * 0.7),
            tick: t.foreground.iced_alpha(fade * 0.85),
            tick_minor: t.muted.iced_alpha(fade * 0.5),
            hand: t.foreground.iced_alpha(fade),
            second: t.accent.iced_alpha(fade),
            show_seconds: t.clock_seconds,
        }
    }
}

impl<Message> canvas::Program<Message> for AnalogClock {
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
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = bounds.width.min(bounds.height) * 0.5 - 2.0;
        // Hand/tick weights scale with the face so a large-text face stays balanced.
        let k = r / 64.0;
        let at = |angle: f32, len: f32| Point::new(c.x + len * angle.sin(), c.y - len * angle.cos());

        // Faint disc so the hands read over a busy sky, then the rim.
        frame.fill(&Path::circle(c, r), self.hand.scale_alpha(0.05));
        frame.stroke(
            &Path::circle(c, r),
            Stroke::default().with_width(1.5 * k).with_color(self.rim),
        );

        // 60 ticks; every fifth (the hours) longer and brighter.
        let tau = std::f32::consts::TAU;
        for i in 0..60 {
            let a = i as f32 / 60.0 * tau;
            let (inner, width, color) = if i % 5 == 0 {
                (r * 0.86, 2.0 * k, self.tick)
            } else {
                (r * 0.92, 1.0 * k, self.tick_minor)
            };
            frame.stroke(
                &Path::line(at(a, inner), at(a, r * 0.98)),
                Stroke::default().with_width(width).with_color(color),
            );
        }

        let (ha, ma, sa) = self.angles;
        // Hour and minute hands (rounded caps), each from a short tail through center.
        frame.stroke(
            &Path::line(at(ha + tau / 2.0, r * 0.10), at(ha, r * 0.50)),
            Stroke::default()
                .with_width(4.5 * k)
                .with_color(self.hand)
                .with_line_cap(LineCap::Round),
        );
        frame.stroke(
            &Path::line(at(ma + tau / 2.0, r * 0.10), at(ma, r * 0.74)),
            Stroke::default()
                .with_width(3.0 * k)
                .with_color(self.hand)
                .with_line_cap(LineCap::Round),
        );
        if self.show_seconds {
            frame.stroke(
                &Path::line(at(sa + tau / 2.0, r * 0.18), at(sa, r * 0.86)),
                Stroke::default()
                    .with_width(1.4 * k)
                    .with_color(self.second)
                    .with_line_cap(LineCap::Round),
            );
        }
        // Center hub.
        frame.fill(&Path::circle(c, 3.0 * k), self.second);

        vec![frame.into_geometry()]
    }
}
