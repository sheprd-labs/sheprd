//! Herdr 2 agents on the badge: one core, however many windows show it.

use super::*;

/// Blocked agents in the terminals `names`, none of them listed.
fn agents(names: &[&str]) -> Vec<CoreAgent> {
    names
        .iter()
        .map(|name| CoreAgent {
            terminal: TerminalId::new(*name).unwrap(),
            status: AgentStatus::Blocked,
            listed: false,
        })
        .collect()
}

fn sync_core(window: WindowId, agents: Vec<CoreAgent>, cx: &mut App) {
    super::sync_core(window, agents, 0, |_| None, cx);
}

#[gpui::test]
fn core_agents_count_once_across_windows_beside_herdr_1(cx: &mut gpui::TestAppContext) {
    let first = cx.add_window(fixture_window);
    let second = cx.add_window(fixture_window);
    first.update(cx, |_, _, cx| install(cx)).unwrap();
    first
        .update(cx, |view, window, cx| {
            let mut state = snapshot(2);
            state.agents[0].agent_status = AgentStatus::Blocked;
            view.endpoints[0].live.snapshot = Some(Arc::new(state));
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            let id = window.window_handle().window_id();
            sync(id, &view.endpoints, cx);
            sync_core(id, agents(&["t1", "t2"]), cx);
            assert_eq!(cx.global::<Badge>().published, Some(3));
        })
        .unwrap();
    second
        .update(cx, |_, window, cx| {
            sync_core(
                window.window_handle().window_id(),
                agents(&["t2", "t3"]),
                cx,
            );
            assert_eq!(cx.global::<Badge>().published, Some(4));
        })
        .unwrap();
    first
        .update(cx, |_, window, cx| {
            sync_core(window.window_handle().window_id(), Vec::new(), cx);
            assert_eq!(cx.global::<Badge>().published, Some(3));
        })
        .unwrap();
}
