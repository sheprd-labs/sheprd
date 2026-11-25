use super::*;
use crate::config::status_bar::Button;

/// Moonframe's status bar draws Theme and Commands as `[status_bar] theme`
/// and `shortcuts` ask, and a changed setting redraws it.
#[gpui::test]
fn the_status_bar_buttons_follow_their_settings(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        let buttons = view.hosted_state(cx).main["status_bar"].clone();
        assert_eq!(buttons, json!({ "commands": "label", "theme": "label" }));

        let before = view.hosted_stamp();
        view.config.status_bar.theme = Button::Hidden;
        view.config.status_bar.shortcuts = Button::Icon;
        assert!(view.hosted_stamp() != before, "the new buttons are drawn");
        let buttons = view.hosted_state(cx).main["status_bar"].clone();
        assert_eq!(buttons, json!({ "commands": "icon", "theme": "hidden" }));
    });
}
