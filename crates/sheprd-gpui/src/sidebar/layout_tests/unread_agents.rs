use super::*;
use crate::{agent_read::Sets, state::ConnectionStatus};

/// An agent with a reply no one read has a dot at its row's end; the same
/// pane on another host is another agent, and has none.
#[gpui::test]
fn unread_agents_have_a_dot(cx: &mut gpui::TestAppContext) {
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            let mut listing = snapshot(1);
            listing.agents.truncate(1);
            listing.agents[0].agent_status = AgentStatus::Idle;
            view.live.snapshot = Some(Arc::new(listing.clone()));
            view.live.status = ConnectionStatus::Connected;
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = Some(Arc::new(listing));
            remote.live.status = ConnectionStatus::Connected;
            view.endpoints.push(remote);
            view.agent_read = Arc::new(Sets::unread(&[(crate::endpoint::LOCAL, &["p0"])]));
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    let row = cx.debug_bounds("agent-local-p0").unwrap();
    let dot = cx.debug_bounds("agent-unread-local-p0").unwrap();
    assert!(row.contains(&dot.center()), "the dot sits on its row");
    assert!(cx.debug_bounds("agent-unread-ssh:test-p0").is_none());
}
