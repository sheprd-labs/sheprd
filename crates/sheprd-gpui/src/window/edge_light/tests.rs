#![allow(clippy::expect_used)]

use super::{AgentStatus, FRAC_PI_2, Paint, Ring, WINDOW_RADIUS};
use gpui::{Bounds, Pixels, Point, point, px, rgb, size};

fn bounds(w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(11.), px(7.)), size(px(w), px(h)))
}

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "{a} != {b}");
}

fn distance(a: Point<Pixels>, b: Point<Pixels>) -> f32 {
    f32::from(a.x - b.x).hypot(f32::from(a.y - b.y))
}

#[test]
fn both_boundaries_join_smoothly_and_share_all_four_corner_centers() {
    let bounds = bounds(691., 303.);
    for width in [1., 5., 12.] {
        let ring = Ring::new(bounds, WINDOW_RADIUS, width).expect("drawable bounds");
        for side in 0..4 {
            for inset in [0., width] {
                let straight_end = ring.point(side, false, 1., inset);
                let next_start = ring.point((side + 1) % 4, false, 0., inset);
                near(
                    distance(straight_end, ring.point(side, true, 0., inset)),
                    0.,
                );
                near(distance(ring.point(side, true, 1., inset), next_start), 0.);
            }
            let center = ring.point(side, true, 0., WINDOW_RADIUS);
            for t in [0., 0.25, 0.5, 0.75, 1.] {
                let outer = ring.point(side, true, t, 0.);
                let inner = ring.point(side, true, t, width);
                near(distance(center, outer), WINDOW_RADIUS);
                near(distance(center, inner), WINDOW_RADIUS - width);
                near(distance(outer, inner), width);
            }
        }
    }
}

#[test]
fn square_fullscreen_ring_miters_without_overlapping_corners() {
    let bounds = bounds(800., 500.);
    let ring = Ring::new(bounds, 0., 8.).expect("drawable bounds");
    assert_eq!(ring.point(0, false, 1., 0.), bounds.top_right());
    let inner_corner = point(bounds.right() - px(8.), bounds.top() + px(8.));
    assert_eq!(ring.point(0, false, 1., 8.), inner_corner);
    for side in 0..4 {
        for inset in [0., 8.] {
            assert_eq!(
                ring.point(side, false, 1., inset),
                ring.point((side + 1) % 4, false, 0., inset),
            );
        }
    }
    ring.spans(0., 1., |_, arc, _, _, _, _| assert!(!arc));
}

#[test]
fn trail_progress_is_continuous_across_the_lap_seam_and_scales_with_length() {
    let ring = Ring::new(bounds(841., 397.), WINDOW_RADIUS, 5.).expect("drawable bounds");
    for trail in [0.05, 0.16, 0.5] {
        let mut previous = None;
        let mut progress = 0.;
        let mut length = 0.;
        ring.spans(0.01 - trail, 0.01, |side, arc, a, b, t0, t1| {
            let start = ring.point(side, arc, a, 0.);
            if let Some(end) = previous {
                near(distance(end, start), 0.);
            }
            previous = Some(ring.point(side, arc, b, 0.));
            near(progress, t0);
            assert!(t1 > t0);
            progress = t1;
            length += (b - a)
                * if arc {
                    FRAC_PI_2 * ring.radius
                } else {
                    ring.side_length(side)
                };
        });
        near(progress, 1.);
        near(length, trail * ring.lap);
    }
}

#[test]
fn narrow_layout_preserves_a_clear_center() {
    let ring = Ring::new(bounds(14., 40.), WINDOW_RADIUS, 12.).expect("drawable bounds");
    near(ring.radius, 7.);
    near(ring.width, 6.5);
    assert!(Ring::new(bounds(0., 100.), WINDOW_RADIUS, 12.).is_none());
}

#[test]
fn custom_color_keeps_two_attention_beats_and_a_quieter_idle_breath() {
    let tint = rgb(0x9324db).into();
    let alpha = |status, phase| {
        Paint {
            status,
            track: tint,
            light: tint,
            radius: WINDOW_RADIUS,
            width: 5.,
            trail: 0.16,
            phase,
        }
        .track_color()
        .a
    };
    let first = alpha(AgentStatus::Blocked, 0.15);
    let second = alpha(AgentStatus::Blocked, 0.42);
    let trough = alpha(AgentStatus::Blocked, 0.29);
    assert!(first > second && second > trough);
    let breath = alpha(AgentStatus::Idle, 0.5);
    assert!(breath > alpha(AgentStatus::Idle, 0.) && breath < trough);
}
