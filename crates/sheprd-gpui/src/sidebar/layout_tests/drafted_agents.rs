use super::*;
use crate::{chat::drafts::Place, state::ConnectionStatus};

/// An agent whose chat holds a draft not sent has a pencil at its row's end,
/// before the unread dot when both show; the same pane on another host is
/// another agent, and has none.
#[gpui::test]
fn drafted_agents_have_a_pencil(cx: &mut gpui::TestAppContext) {
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
            view.agent_read = Arc::new(crate::agent_read::Sets::unread(&[(
                crate::endpoint::LOCAL,
                &["p0"],
            )]));
            view.chat_drafts = Arc::new(
                [Place::Herdr {
                    endpoint: crate::endpoint::LOCAL.to_owned(),
                    pane: "p0".to_owned(),
                }]
                .into(),
            );
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
    let pencil = cx.debug_bounds("agent-drafted-local-p0").unwrap();
    let dot = cx.debug_bounds("agent-unread-local-p0").unwrap();
    assert!(row.contains(&pencil.center()), "the pencil sits on its row");
    assert!(
        pencil.right() <= dot.left(),
        "the pencil comes before the dot"
    );
    assert!(cx.debug_bounds("agent-drafted-ssh:test-p0").is_none());
}

/// A drafted agent's text ends before its pencil and the unread dot after
/// it, rather than running under them.
#[gpui::test]
fn a_drafted_row_keeps_its_text_clear_of_the_marks(cx: &mut gpui::TestAppContext) {
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            let mut listing = snapshot(1);
            listing.agents.truncate(1);
            listing.agents[0].agent_status = AgentStatus::Idle;
            listing.agents[0].display_agent = Some("an agent with a very long name ".repeat(8));
            view.live.snapshot = Some(Arc::new(listing));
            view.live.status = ConnectionStatus::Connected;
            view.agent_read = Arc::new(crate::agent_read::Sets::unread(&[(
                crate::endpoint::LOCAL,
                &["p0"],
            )]));
            view.chat_drafts = Arc::new(
                [Place::Herdr {
                    endpoint: crate::endpoint::LOCAL.to_owned(),
                    pane: "p0".to_owned(),
                }]
                .into(),
            );
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
    let pencil = cx.debug_bounds("agent-drafted-local-p0").unwrap();
    let column = cx.debug_bounds("column-agent-p0").unwrap();
    assert!(
        column.right() <= pencil.left(),
        "the text ends before the pencil: {column:?} {pencil:?}"
    );
}
