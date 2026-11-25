use super::{Answer, Answered, Ask};

#[test]
fn closing_unanswered_declines_and_the_first_answer_stays() {
    let answered = Answered::default();
    let ask = Ask {
        host: "mini-2".to_owned(),
        key: "mini-2".to_owned(),
        machine: "default".to_owned(),
        answered: answered.clone(),
    };
    drop(ask);
    assert_eq!(answered.take(), Some(Answer::Dismissed));
    assert_eq!(answered.take(), None);
    let answered = Answered::default();
    answered.set(Answer::Once);
    answered.set(Answer::Dismissed);
    assert_eq!(answered.take(), Some(Answer::Once));
}

#[test]
fn a_cancelled_request_is_seen_by_its_question_only() {
    let answered = Answered::default();
    let other = Answered::default();
    answered.clone().cancel();
    assert!(answered.cancelled());
    assert!(!other.cancelled());
    assert!(answered.same(&answered.clone()));
    assert!(!answered.same(&other));
}

#[gpui::test]
fn a_borrow_request_waits_for_a_click_without_taking_focus(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let original = cx.focus_handle();
            window.focus(&original, cx);
            let ask = view.ask_to_borrow("mini".into(), "mini".into(), "work".into(), window, cx);
            assert!(original.is_focused(window));
            assert!(view.menu.page.is_none());
            assert!(view.notice_shown(crate::window::NoticeCard::BorrowMachine));
            let Some(ask) = ask else {
                panic!("expected a borrow request")
            };
            assert!(view.open_menu(window, cx));
            view.dismiss_menu(window, cx);
            assert!(ask.take().is_none());
            assert!(view.notice_shown(crate::window::NoticeCard::BorrowMachine));
            assert!(view.open_menu(window, cx));
            view.menu.page = Some(crate::menu::Page::BorrowMachine);
            view.dismiss_menu(window, cx);
            assert_eq!(ask.take(), Some(Answer::Dismissed));
        })
    });
}
