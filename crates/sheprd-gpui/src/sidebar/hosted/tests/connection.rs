use super::*;
use crate::state::ConnectionStatus;

/// Moonframe's status bar reads the classic bar's own connection message,
/// so a rule that silences one silences both: `null` while there is none.
#[gpui::test]
fn the_status_is_the_classic_bars_connection_message(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.live.status = ConnectionStatus::Connected;
        view.live.error = None;
        view.local_error = None;
        assert_eq!(view.connection_message(), None);
        assert_eq!(view.hosted_state(cx).main["status"], Value::Null);

        let before = view.hosted_stamp();
        view.live.status = ConnectionStatus::Disconnected;
        view.live.error = Some("socket closed".into());
        let message = view.connection_message();
        assert!(message.is_some(), "a Herdr 1 the user runs reports it");
        assert_eq!(view.hosted_state(cx).main["status"], json!(message));
        assert!(view.hosted_stamp() != before, "a new message is drawn");
    });
}
