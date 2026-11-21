//! Settings > Remote control > the push relay: turning it on asks first
//! with what the push relay sees, turning it off does not, and the page
//! shows how the daemon's pushes leave it and how the last ones went.
use super::{
    remote_control::{click, draw, page},
    *,
};
use crate::{
    config::{preferences::Preference, remote::Edit},
    remote::Supervisor,
    settings_window::pages::remote_control::{delivery_line, last_alert, last_live, last_worked},
};
use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};

/// `text` without Fluent's isolation marks around its arguments.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn last(at_ms: u64, outcome: Outcome) -> Option<Last> {
    Some(Last { at_ms, outcome })
}

#[gpui::test]
fn turning_the_push_relay_on_asks_first(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    click(cx, "remote-push-relay-toggle");
    assert!(
        edits.lock().unwrap().is_empty(),
        "nothing saved before asking"
    );
    view.read_with(cx, |view, _| assert!(view.remote.push_relay.is_open()));
    // The explanation names what the push relay sees and never sees.
    assert!(cx.debug_bounds("remote-push-relay-ask").is_some());
    let english = crate::i18n::english("settings-push-relay-ask-never", &[]);
    assert!(english.contains("prompts, chats, code"));

    click(cx, "remote-push-relay-cancel");
    view.read_with(cx, |view, _| assert!(!view.remote.push_relay.is_open()));
    assert!(edits.lock().unwrap().is_empty());

    click(cx, "remote-push-relay-toggle");
    click(cx, "remote-push-relay-confirm");
    view.read_with(cx, |view, _| assert!(!view.remote.push_relay.is_open()));
    assert_eq!(
        *edits.lock().unwrap(),
        [Preference::Remote(Edit::PushRelay(true))]
    );
}

#[gpui::test]
fn turning_the_push_relay_off_needs_no_asking(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    view.update(cx, |view, cx| {
        view.config.remote.push_relay = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "remote-push-relay-toggle");
    view.read_with(cx, |view, _| assert!(!view.remote.push_relay.is_open()));
    assert_eq!(
        *edits.lock().unwrap(),
        [Preference::Remote(Edit::PushRelay(false))]
    );
}

#[gpui::test]
fn the_page_shows_how_pushes_leave_and_how_the_last_went(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    assert!(cx.debug_bounds("remote-push-delivery-last").is_none());
    let now = crate::remote::now_ms();
    cx.update(|_, cx| {
        Supervisor::set_push_status(
            cx,
            Some(PushStatus {
                mode: Mode::PushRelay,
                alert: last(now, Outcome::Delivered),
                live_activity: None,
            }),
        );
    });
    view.update(cx, |_, cx| cx.notify());
    draw(cx);
    assert!(cx.debug_bounds("remote-push-delivery-last").is_some());
    assert!(cx.debug_bounds("remote-push-delivery-dot").is_some());
}

#[test]
fn delivery_lines_name_the_mode_and_each_outcome() {
    assert_eq!(
        delivery_line(None, true),
        "Shown while remote control runs."
    );
    let own = PushStatus {
        mode: Mode::OwnKey,
        ..PushStatus::default()
    };
    assert!(delivery_line(Some(&own), true).contains("wins over the push relay"));
    assert!(!delivery_line(Some(&own), false).contains("push relay"));
    assert_eq!(
        delivery_line(Some(&PushStatus::default()), false),
        "Off: there is no APNs key here, and the push relay is off."
    );

    assert_eq!(last_alert(None, 0), "No alert since the daemon started.");
    assert_eq!(
        plain(last_alert(last(0, Outcome::Delivered).as_ref(), 0)),
        "Last alert delivered just now."
    );
    let refused = Outcome::Refused {
        status: 403,
        reason: None,
    };
    assert_eq!(
        plain(last_live(last(0, refused).as_ref(), 120_000)),
        "Last Live Activity push 2 min ago: refused (403)."
    );
}

#[test]
fn the_dot_follows_the_most_recent_push() {
    assert_eq!(last_worked(&PushStatus::default()), None);
    let status = PushStatus {
        mode: Mode::PushRelay,
        alert: last(5, Outcome::Delivered),
        live_activity: last(9, Outcome::Unreachable),
    };
    assert_eq!(last_worked(&status), Some(false));
}
