use super::*;
use crate::agent_read::Sets;

/// Moonframe's agents list hears which agents have a reply no one read, and
/// a new answer from a daemon builds its state again.
#[gpui::test]
fn unread_agents_are_marked_and_restamp_the_state(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.live.snapshot = Some(Arc::new(crate::sidebar::layout_tests::snapshot(2)));
        let before = view.hosted_stamp();
        view.agent_read = Arc::new(Sets::unread(&[(crate::endpoint::LOCAL, &["p0"])]));
        assert!(view.hosted_stamp() != before, "a new unread set is drawn");
        let state = view.hosted_state(cx);
        let agents = state.main["agents"].as_array().unwrap();
        let unread = |pane: &str| {
            agents
                .iter()
                .find(|agent| agent["pane"] == pane)
                .map(|agent| agent["unread"].clone())
        };
        assert_eq!(unread("p0"), Some(Value::Bool(true)));
        assert_eq!(unread("p1"), Some(Value::Bool(false)));
        assert!(state.theme["unread"].is_string(), "the dot's colour");
    });
}
