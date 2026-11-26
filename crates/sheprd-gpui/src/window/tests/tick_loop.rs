use super::*;
use crate::core_terminals::{Host, Purpose};
use std::time::Duration;

/// A tick that comes due while the window is busy, as in a nested run loop,
/// must not end the ticking: results that only a tick takes, such as a
/// started terminal or a close dialog's check, would then wait forever.
#[gpui::test]
fn ticking_resumes_after_a_tick_lands_on_a_busy_window(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view._poll = SheprdWindow::tick_every_frame(window, cx);
            let purpose = Purpose::Tab {
                focus: false,
                group: None,
                scope: crate::browser::scope(&view.endpoints[0]),
                workspace: "workspace".into(),
            };
            view.core_terminals.start_for_test(
                purpose,
                Host::Local,
                Err(sheprd_core_client::Error::Lost),
            );
        });
    });

    // The timer fires while the window is still being updated.
    cx.update(|_, cx| {
        cx.background_executor()
            .advance_clock(Duration::from_millis(16));
        cx.background_executor().run_until_parked();
    });
    assert!(
        view.read_with(cx, |view, _| view.core_terminals.is_starting()),
        "a busy window cannot tick, so the start still waits"
    );

    // Once the window is free again, the next tick places the start.
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    assert!(
        !view.read_with(cx, |view, _| view.core_terminals.is_starting()),
        "ticking stopped for good after one busy tick"
    );
}
