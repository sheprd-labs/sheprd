//! A passive status light. Track and traveling light share one inset ring;
//! nothing fills the window or registers a hitbox.

use crate::config::EdgeLight;
use gpui::{prelude::*, *};
use sheprd_client::protocol::AgentStatus;
use std::{
    f32::consts::{FRAC_PI_2, TAU},
    time::Duration,
};

// Fitted to the macOS window mask in logical pixels (including its tapered ends).
const WINDOW_RADIUS: f32 = 18.;

pub(super) fn frame(status: AgentStatus, settings: EdgeLight, rounded: bool) -> AnyElement {
    if status == AgentStatus::Unknown {
        return Empty.into_any_element();
    }
    Layer {
        status,
        settings,
        rounded,
    }
    .into_any_element()
}

#[derive(Clone, Copy, PartialEq, IntoElement)]
struct Layer {
    status: AgentStatus,
    settings: EdgeLight,
    rounded: bool,
}

impl RenderOnce for Layer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Animation notifies its current view. Retain a child view without
        // observing it: its timer must not notify SheprdWindow or a Luau host.
        window.with_global_id("edge-light-layer".into(), |id, window| {
            window.with_element_state(id, |state: Option<Entity<Self>>, _| {
                let layer = state.unwrap_or_else(|| cx.new(|_| self));
                if *layer.read(cx) != self {
                    layer.update(cx, |layer, _| *layer = self);
                }
                (layer.clone(), layer)
            })
        })
    }
}

impl Render for Layer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (ms, fps, tint, highlight, depth) = match self.status {
            AgentStatus::Working => (4000, crate::mark_layer::MAX_FPS, 0x63baff, 0xd7f4ff, 1.),
            AgentStatus::Blocked => (3200, 20., 0xffad53, 0xffa042, 0.8),
            AgentStatus::Done => (3960, crate::mark_layer::MAX_FPS, 0x67d9a6, 0x68f6b8, 0.8),
            AgentStatus::Idle => (6400, 10., 0x90a5bb, 0x97b4cb, 0.4),
            AgentStatus::Unknown => return Empty.into_any_element(),
        };
        let frame = Paint {
            status: self.status,
            track: rgb(self.settings.color.unwrap_or(tint)).into(),
            light: rgb(self.settings.color.unwrap_or(highlight)).into(),
            radius: if self.rounded { WINDOW_RADIUS } else { 0. },
            width: (f32::from(self.settings.width) * depth).max(1.),
            trail: f32::from(self.settings.trail) / 100.,
            phase: 0.,
        };
        frame
            .with_animation(
                "edge-light-phase",
                Animation::new(crate::motion::duration(Duration::from_millis(ms)))
                    .repeat_synced()
                    .with_max_fps(fps),
                |frame, phase| Paint { phase, ..frame },
            )
            .into_any_element()
    }
}

#[derive(IntoElement)]
struct Paint {
    status: AgentStatus,
    track: Hsla,
    light: Hsla,
    radius: f32,
    width: f32,
    trail: f32,
    phase: f32,
}

impl RenderOnce for Paint {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| self.paint(bounds, window),
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

impl Paint {
    fn track_color(&self) -> Hsla {
        let (track, pulse) = match self.status {
            AgentStatus::Working => (0.20, 0.),
            AgentStatus::Done => (0.28, 0.),
            AgentStatus::Blocked => {
                let beat = |at: f32, width: f32| (-((self.phase - at) / width).powi(2)).exp();
                (
                    0.28,
                    0.12 + 0.74 * (beat(0.15, 0.075) + 0.72 * beat(0.42, 0.08)),
                )
            }
            AgentStatus::Idle => (0.12, 0.067 + 0.059 * (1. - (TAU * self.phase).cos())),
            AgentStatus::Unknown => return transparent_black(),
        };
        // Hsla::blend mixes colors, preserving the base alpha. Composite
        // the two opacities explicitly so a custom tint still pulses.
        let opacity = pulse + track * (1. - pulse);
        self.track
            .blend(self.light.opacity(pulse / opacity))
            .opacity(opacity)
    }

    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let Some(ring) = Ring::new(bounds, self.radius, self.width) else {
            return;
        };
        window.paint_quad(
            outline(bounds, self.track_color(), BorderStyle::default())
                .corner_radii(px(ring.radius))
                .border_widths(px(ring.width)),
        );
        let mut ramp = |a, b, low, high| ring.ramp(a, b, low, high, self.light, window);
        if self.status == AgentStatus::Working {
            let head = self.phase + 0.11;
            let shoulder = head - 0.32 * self.trail;
            ramp(head - self.trail, shoulder, 0., 0.2);
            ramp(shoulder, head, 0.2, 1.);
            ramp(head, head + 0.08 * self.trail, 1., 0.);
        } else if self.status == AgentStatus::Done {
            // Split at top center and meet at bottom center. Each half
            // owns its interval, even as the fronts part and reunite.
            let origin = ring.side_length(0) * 0.5 / ring.lap;
            let spread = self.trail * 0.5;
            let head = 0.84 * self.phase;
            let rising = (head - spread, head, 0., 0.88);
            let falling = (head, head + spread, 0.88, 0.);
            for (a, b, low, high) in [rising, falling] {
                let start = a.max(0.);
                let end = b.min(0.5);
                if end <= start {
                    continue;
                }
                let alpha = |t| low + (high - low) * (t - a) / spread;
                ramp(origin + start, origin + end, alpha(start), alpha(end));
                ramp(origin - end, origin - start, alpha(end), alpha(start));
            }
        }
    }
}

/// Clockwise arclength starting at the top left tangent. Each side owns
/// its straight run and the circular arc turning into the following side.
struct Ring {
    bounds: Bounds<Pixels>,
    radius: f32,
    width: f32,
    lap: f32,
}

impl Ring {
    fn new(bounds: Bounds<Pixels>, radius: f32, width: f32) -> Option<Self> {
        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        // Even a transient, tiny layout must leave a clear center.
        let width = width.min((w.min(h) - 1.) * 0.5);
        if width <= 0. {
            return None;
        }
        let radius = radius.min(w.min(h) * 0.5);
        let lap = 2. * (w + h - 4. * radius) + TAU * radius;
        Some(Self {
            bounds,
            radius,
            width,
            lap,
        })
    }

    fn extent(&self, side: usize) -> f32 {
        if side.is_multiple_of(2) {
            self.bounds.size.width.into()
        } else {
            self.bounds.size.height.into()
        }
    }

    fn side_length(&self, side: usize) -> f32 {
        self.extent(side) - 2. * self.radius
    }

    /// The same normalized position on the outer or inner boundary. Insets
    /// subtract from the radius, not the arc center. With square corners a
    /// side ends on the miter diagonal: adjacent sides share that edge.
    fn point(&self, side: usize, arc: bool, t: f32, inset: f32) -> Point<Pixels> {
        let r = (self.radius - inset).max(0.);
        let tangent = inset + r;
        let (u, v) = if arc {
            let (sin, cos) = (FRAC_PI_2 * t).sin_cos();
            (self.extent(side) - tangent + r * sin, tangent - r * cos)
        } else {
            (tangent + (self.extent(side) - 2. * tangent) * t, inset)
        };
        let b = self.bounds;
        match side {
            0 => point(b.left() + px(u), b.top() + px(v)),
            1 => point(b.right() - px(v), b.top() + px(u)),
            2 => point(b.right() - px(u), b.bottom() - px(v)),
            _ => point(b.left() + px(v), b.bottom() - px(u)),
        }
    }

    /// Break a ramp only where the direction of the perimeter changes. The
    /// last arc connects directly to the first straight, including for a
    /// trail that straddles phase zero.
    fn spans(&self, start: f32, end: f32, mut visit: impl FnMut(usize, bool, f32, f32, f32, f32)) {
        let length = (end - start) * self.lap;
        if length <= 0. {
            return;
        }
        let start = start.rem_euclid(1.) * self.lap;
        let end = start + length;
        for offset in [0., self.lap] {
            let mut cursor = offset;
            for side in 0..4 {
                for (arc, len) in [
                    (false, self.side_length(side)),
                    (true, FRAC_PI_2 * self.radius),
                ] {
                    if cursor >= end {
                        return;
                    }
                    let a = start.max(cursor);
                    let b = end.min(cursor + len);
                    if b > a {
                        visit(
                            side,
                            arc,
                            (a - cursor) / len,
                            (b - cursor) / len,
                            (a - start) / length,
                            (b - start) / length,
                        );
                    }
                    cursor += len;
                }
            }
        }
    }

    fn ramp(&self, start: f32, end: f32, low: f32, high: f32, color: Hsla, window: &mut Window) {
        let stop = |t, pos| linear_color_stop(color.opacity(low + (high - low) * t), pos);
        self.spans(start, end, |side, arc, a, b, t0, t1| {
            let angle = (side + 1) as f32 * 90. + if arc { 45. * (a + b) } else { 0. };
            let gradient = linear_gradient(angle, stop(t0, 0.), stop(t1, 1.));
            let p = |t, inset| self.point(side, arc, t, inset);
            let outer_a = p(a, 0.);
            let inner_b = p(b, self.width);
            if !arc && self.radius >= self.width {
                // The long straight runs need only one gradient quad each.
                let rect = Bounds::from_corners(
                    point(outer_a.x.min(inner_b.x), outer_a.y.min(inner_b.y)),
                    point(outer_a.x.max(inner_b.x), outer_a.y.max(inner_b.y)),
                );
                window.paint_quad(fill(rect, gradient));
                return;
            }
            let mut path = PathBuilder::fill();
            path.move_to(outer_a);
            let outer_b = p(b, 0.);
            if arc {
                let r = px(self.radius);
                path.arc_to(point(r, r), px(0.), false, true, outer_b);
            } else {
                path.line_to(outer_b);
            }
            path.line_to(inner_b);
            let inner_a = p(a, self.width);
            if arc && self.radius > self.width {
                let r = px(self.radius - self.width);
                path.arc_to(point(r, r), px(0.), false, false, inner_a);
            } else {
                path.line_to(inner_a);
            }
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, gradient);
            }
        });
    }
}

#[cfg(test)]
mod tests;
