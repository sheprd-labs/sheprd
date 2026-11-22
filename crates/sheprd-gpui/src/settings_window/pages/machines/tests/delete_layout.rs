use super::*;

/// The long question wraps under its title as the row's description, ending
/// before the buttons, which stay at the row's right edge in line with the
/// machine's other buttons, even in a narrow window.
#[gpui::test]
fn the_delete_question_wraps_and_its_buttons_keep_their_place(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx, vec![listing("one", false)], Some("one"));
    cx.simulate_resize(gpui::size(gpui::px(640.), gpui::px(2000.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let edge = cx.debug_bounds("machine-one-cpus-more").unwrap().right();
    let question = cx
        .debug_bounds("machine-one-delete-ask-description")
        .unwrap();
    let cancel = cx.debug_bounds("machine-one-delete-cancel").unwrap();
    let confirm = cx.debug_bounds("machine-one-delete-confirm").unwrap();
    assert_eq!(cancel.right(), edge);
    assert!(confirm.right() <= cancel.left(), "{confirm:?} {cancel:?}");
    assert!(
        question.right() <= confirm.left(),
        "{question:?} {confirm:?}"
    );
    assert!(
        question.size.height > gpui::px(15.),
        "one line: {question:?}"
    );
}
