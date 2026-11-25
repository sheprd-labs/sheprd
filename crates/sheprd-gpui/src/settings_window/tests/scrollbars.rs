//! A pane whose content runs past it shows a thumb that follows its scroll
//! and drags it; one that fits shows none.
use super::*;
use gpui::{px, size};

#[gpui::test]
fn overflowing_panes_show_a_thumb_that_drags_them(cx: &mut TestAppContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|_, cx| SettingsWindow::new(weak, cx));
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };

    // Tall enough for every category: the section list has no thumb.
    cx.simulate_resize(size(px(900.), px(1400.)));
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("settings-sections-thumb").is_none());

    cx.simulate_resize(size(px(680.), px(420.)));
    // The thumb reads the layout before it, so a resize shows it next frame.
    draw(cx);
    draw(cx);
    for (pane, area) in [
        ("settings-sections-thumb", "settings-sections"),
        ("settings-body-thumb", "settings-body"),
    ] {
        let thumb = cx.debug_bounds(pane).unwrap_or_else(|| panic!("{pane}"));
        let area = cx.debug_bounds(area).unwrap();
        assert!(thumb.size.height < area.size.height, "{pane}: {thumb:?}");
        assert!(
            (thumb.top() - area.top()).abs() < px(1.),
            "{pane} starts at the top"
        );
        let expected_right = if pane == "settings-sections-thumb" {
            // The navigation list is padded, but its thumb belongs at the
            // panel edge rather than beside the padded row content.
            cx.debug_bounds("settings-navigation-resize")
                .unwrap()
                .right()
                - px(4.)
        } else {
            area.right() - px(3.)
        };
        assert!(
            (thumb.right() - expected_right).abs() <= px(1.),
            "{pane} is aligned: thumb={thumb:?}, area={area:?}, expected_right={expected_right:?}"
        );
    }

    // The sections thumb lives beyond the padded list bounds, so verify that
    // it still owns the navigation scroll gesture rather than only painting
    // in the expected place.
    let thumb = cx.debug_bounds("settings-sections-thumb").unwrap();
    // The resize grip owns the few pixels where the panel edge and the thumb
    // meet; begin on the thumb's scrollable side.
    let from = point(thumb.left() + px(1.), thumb.center().y);
    let to = from + point(px(0.), px(120.));
    cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
    draw(cx);
    let navigation_scrolled = view.read_with(cx, |view, _| view.navigation_scroll.offset().y);
    assert!(
        navigation_scrolled < px(0.),
        "the sections list scrolled: {navigation_scrolled:?}"
    );

    // Dragging the body's thumb down scrolls the body, and the thumb follows.
    let thumb = cx.debug_bounds("settings-body-thumb").unwrap();
    let from = thumb.center();
    let to = from + point(px(0.), px(120.));
    cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
    draw(cx);
    let scrolled = view.read_with(cx, |view, _| view.body_scroll.offset().y);
    assert!(scrolled < px(0.), "the body scrolled: {scrolled:?}");
    let moved = cx.debug_bounds("settings-body-thumb").unwrap();
    assert!(moved.top() > thumb.top(), "{moved:?} after {thumb:?}");
}
