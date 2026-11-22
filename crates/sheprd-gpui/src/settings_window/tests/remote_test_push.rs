//! Settings > Remote control's Send Test Push: shown while the push relay
//! is on, it asks the daemon and says what came of each phone's alert.
use super::{
    remote_control::{click, draw, page},
    *,
};
use crate::settings_window::pages::remote_control::{TestPushState, answered, test_push_lines};
use sheprd_remote_control::control::push::{Mode, Outcome, TestPush, TestPushes};

/// `text` without Fluent's isolation marks around its arguments.
fn plain(text: &str) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

#[gpui::test]
fn the_button_asks_the_daemon_and_shows_the_answer(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    assert!(cx.debug_bounds("remote-test-push").is_some());
    // The fixture's daemon answers `Done`, which no test push is.
    click(cx, "remote-test-push");
    view.read_with(cx, |view, _| {
        assert!(
            matches!(view.remote.test_push, TestPushState::Failed(_)),
            "{:?}",
            view.remote.test_push
        );
    });
    assert!(cx.debug_bounds("remote-test-push-result").is_some());
}

#[gpui::test]
fn with_the_push_relay_off_there_is_no_button(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    view.update(cx, |view, cx| {
        view.config.remote.push_relay = false;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("remote-test-push").is_none());
}

#[test]
fn each_phone_gets_a_line_or_the_page_says_why_none_went() {
    assert!(test_push_lines(&TestPushState::Idle).is_empty());
    let sent = TestPushState::Sent(TestPushes {
        mode: Mode::PushRelay,
        pushes: vec![
            TestPush {
                device: "Work iPhone".into(),
                outcome: Outcome::Delivered,
            },
            TestPush {
                device: "Old iPhone".into(),
                outcome: Outcome::Refused {
                    status: 403,
                    reason: None,
                },
            },
        ],
    });
    let lines: Vec<String> = test_push_lines(&sent).iter().map(|l| plain(l)).collect();
    assert_eq!(
        lines,
        [
            "Work iPhone: Apple took it; it should show on the phone now.",
            "Old iPhone: refused (403).",
        ]
    );
    let none = test_push_lines(&TestPushState::Sent(TestPushes {
        mode: Mode::PushRelay,
        pushes: Vec::new(),
    }));
    assert!(plain(&none[0]).contains("no paired iPhone"));
    let off = test_push_lines(&TestPushState::Sent(TestPushes::default()));
    assert!(plain(&off[0]).contains("push relay off"));
}

#[test]
fn an_older_daemon_is_told_to_restart() {
    let state = answered(Err(sheprd_remote_control::control::Error::NoAnswer));
    let lines = test_push_lines(&state);
    assert!(
        plain(&lines[0]).contains("Restart remote control"),
        "{lines:?}"
    );
}

#[gpui::test]
fn an_answer_after_leaving_the_page_is_dropped(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    view.update(cx, |view, cx| {
        view.send_test_push(cx);
        // The page goes away before the daemon answers.
        view.close_remote_control();
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(
            matches!(view.remote.test_push, TestPushState::Idle),
            "{:?}",
            view.remote.test_push
        );
    });
}
