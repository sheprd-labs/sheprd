use super::{changes, window};

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// A laptop-sized window gives the diff all its room: both side panels
/// start hidden, as on a small screen, until the header's icons ask.
#[gpui::test]
fn a_laptop_wide_review_opens_on_the_diff_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.simulate_resize(gpui::size(gpui::px(1280.), gpui::px(800.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.seed_review(changes(), window, cx);
        })
    });
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("review-files").is_none());
    assert!(cx.debug_bounds("review-notes").is_none());
    assert!(cx.debug_bounds("review-toggle-files").is_some());
}
