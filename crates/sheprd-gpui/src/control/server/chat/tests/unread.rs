use super::*;
use crate::{agent_read::AgentRead, control::protocol::app::Switch, sidebar::layout_tests};
use std::sync::Arc;

fn unread(switch: Switch, pane: &str) -> ChatOp {
    ChatOp::Unread {
        switch,
        pane: Some(pane.into()),
    }
}

#[gpui::test]
fn a_pane_agent_is_marked_unread_and_read_on_its_host(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        AgentRead::install(cx);
        view.update(cx, |view, _| {
            view.live.snapshot = Some(Arc::new(layout_tests::snapshot(1)));
        });
        for (switch, expected) in [
            (Switch::On, true),
            (Switch::On, true),
            (Switch::Toggle, false),
            (Switch::Toggle, true),
            (Switch::Off, false),
        ] {
            assert_eq!(
                serve(call(unread(switch, "p0")), cx),
                Response::Chat {
                    reply: Reply::Unread {
                        pane_id: "p0".into(),
                        unread: expected,
                    }
                },
                "{switch:?}"
            );
            // The window hears of it as the panel does.
            let shown = AgentRead::current(cx);
            view.update(cx, |view, _| view.agent_read = shown.clone());
            assert_eq!(
                shown.contains(crate::endpoint::LOCAL, "p0"),
                expected,
                "{switch:?}"
            );
        }
    });
}

#[gpui::test]
fn a_pane_without_an_agent_cannot_be_marked(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        AgentRead::install(cx);
        view.update(cx, |view, _| {
            view.live.snapshot = Some(Arc::new(layout_tests::snapshot(1)));
        });
        assert_eq!(
            code(&serve(call(unread(Switch::On, "no-such-pane")), cx)),
            Some(ErrorCode::NotFound)
        );
    });
}
