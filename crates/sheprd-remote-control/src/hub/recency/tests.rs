use super::*;

fn seen(times: &mut AgentTimes, agents: &[(&str, u64)], now_ms: u64) {
    times.observe("boot-1", agents.iter().copied(), now_ms);
}

#[test]
fn agents_already_there_have_no_known_time() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    assert_eq!(times.changed_ms("p1"), None);
}

#[test]
fn a_moved_sequence_is_dated_and_keeps_that_time() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    seen(&mut times, &[("p1", 8)], 200);
    assert_eq!(times.changed_ms("p1"), Some(200));
    seen(&mut times, &[("p1", 8)], 300);
    assert_eq!(times.changed_ms("p1"), Some(200));
}

#[test]
fn an_agent_that_appears_later_just_started() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    seen(&mut times, &[("p1", 7), ("p2", 1)], 200);
    assert_eq!(times.changed_ms("p2"), Some(200));
}

#[test]
fn a_new_boot_does_not_read_as_every_agent_changing() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    seen(&mut times, &[("p1", 8)], 200);
    times.observe("boot-2", [("p1", 1)], 300);
    assert_eq!(times.changed_ms("p1"), None);
}

#[test]
fn agents_that_left_are_forgotten() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7), ("p2", 7)], 100);
    seen(&mut times, &[("p1", 8), ("p2", 8)], 200);
    seen(&mut times, &[("p1", 8)], 300);
    assert_eq!(times.panes.len(), 1);
    assert_eq!(times.changed_ms("p2"), None);
}

fn core_layout(status: sheprd_client::protocol::AgentStatus) -> Layout {
    use crate::hub::{Terminal, TerminalAgent, Workspace};
    Layout {
        workspaces: vec![Workspace {
            id: "w".into(),
            label: "w".into(),
            branch: None,
            cwd: "/w".into(),
            session: None,
            worktree: None,
            chats: Vec::new(),
            terminals: vec![Terminal {
                tab: "t".into(),
                label: String::new(),
                cwd: None,
                panes: Vec::new(),
                zoomed: false,
                agent: Some(TerminalAgent {
                    name: None,
                    status,
                    pane: "core:x-61".into(),
                    title: None,
                    changed_ms: None,
                }),
                agents: Vec::new(),
                convert: None,
            }],
        }],
        ..Layout::default()
    }
}

fn changed(layout: &Layout) -> Option<u64> {
    layout.workspaces[0].terminals[0].agent.as_ref()?.changed_ms
}

#[test]
fn a_core_agent_is_dated_when_its_status_moves() {
    use sheprd_client::protocol::AgentStatus;
    let mut times = AgentTimes::default();
    let mut layout = core_layout(AgentStatus::Done);
    times.stamp_statuses("core", &mut layout, 10);
    assert_eq!(changed(&layout), None, "already done when first seen");
    let mut layout = core_layout(AgentStatus::Working);
    times.stamp_statuses("core", &mut layout, 20);
    assert_eq!(changed(&layout), Some(20));
    let mut layout = core_layout(AgentStatus::Done);
    times.stamp_statuses("core", &mut layout, 30);
    assert_eq!(changed(&layout), Some(30));
    times.stamp_statuses("core", &mut layout, 40);
    assert_eq!(changed(&layout), Some(30), "the same status keeps its time");
}
