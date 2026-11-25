use super::*;
use crate::chat::drafts::Place;

/// Moonframe's agents list hears which chats hold a draft not sent, and a
/// pencil that comes builds its state again.
#[gpui::test]
fn drafted_agents_are_marked_and_restamp_the_state(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.live.snapshot = Some(Arc::new(crate::sidebar::layout_tests::snapshot(2)));
        let before = view.hosted_stamp();
        view.chat_drafts = Arc::new(
            [Place::Herdr {
                endpoint: crate::endpoint::LOCAL.to_owned(),
                pane: "p1".to_owned(),
            }]
            .into(),
        );
        assert!(view.hosted_stamp() != before, "a new pencil is drawn");
        let state = view.hosted_state(cx);
        let agents = state.main["agents"].as_array().unwrap();
        let drafted = |pane: &str| {
            agents
                .iter()
                .find(|agent| agent["pane"] == pane)
                .map(|agent| agent["drafted"].clone())
        };
        assert_eq!(drafted("p0"), Some(Value::Bool(false)));
        assert_eq!(drafted("p1"), Some(Value::Bool(true)));
    });
}
