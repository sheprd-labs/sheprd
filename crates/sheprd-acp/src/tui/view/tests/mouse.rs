#![allow(clippy::unwrap_used, clippy::expect_used)]

use crossterm::event::MouseEventKind;

use super::*;

#[test]
fn wheel_scrolls_the_transcript_without_changing_the_composer() {
    let mut view = View::default();
    view.composer.text = "draft prompt".to_owned();

    view.mouse(MouseEventKind::ScrollUp);
    assert_eq!(view.scroll, 3);
    assert_eq!(view.composer.text, "draft prompt");

    view.mouse(MouseEventKind::ScrollDown);
    assert_eq!(view.scroll, 0);
    assert_eq!(view.composer.text, "draft prompt");
}

#[test]
fn wheel_scroll_is_saturating_and_ignores_other_mouse_events() {
    let mut view = View::default();
    view.mouse(MouseEventKind::ScrollDown);
    assert_eq!(view.scroll, 0);

    view.scroll = usize::MAX - 1;
    view.mouse(MouseEventKind::ScrollUp);
    assert_eq!(view.scroll, usize::MAX);

    view.mouse(MouseEventKind::Moved);
    assert_eq!(view.scroll, usize::MAX);
}
