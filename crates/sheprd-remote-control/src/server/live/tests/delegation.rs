//! Exercise the notifier entry point, including discovery of a delegated
//! chat that already started by the time its snapshot arrives.
use super::*;
use crate::hub::{ChatRow, Hub, Layout, Workspace};
use sheprd_chat_protocol::{Entry, EntryId, Turn, TurnEnd, TurnSummary};

fn workspace(pane: &str, endpoint: EndpointId) -> Workspace {
    Workspace {
        id: pane.into(),
        label: pane.into(),
        branch: None,
        cwd: String::new(),
        worktree: None,
        session: None,
        chats: vec![ChatRow {
            pane: pane.into(),
            tab: pane.into(),
            endpoint,
            agent: None,
            title: "Codex".into(),
            named: false,
            status: sheprd_client::protocol::AgentStatus::Working,
        }],
        terminals: vec![],
    }
}

#[test]
fn a_child_in_another_space_updates_only_its_parent() {
    let parent = EndpointId::random();
    let child = EndpointId::random();
    let hub = Hub::offline(Layout {
        connected: true,
        workspaces: vec![
            workspace("parent", parent.clone()),
            workspace("child", child.clone()),
        ],
        ..Layout::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let mut state = crate::web::tests::state_with(dir.path(), hub);
    Arc::get_mut(&mut state).unwrap().apns = Delivery::new(true).unwrap();
    let mut live = LiveActivities::default();
    let turn = Some(Turn { started_ms: 1000 });
    live.change(
        &state,
        "parent",
        &Transcript::new("Codex"),
        &Change::Turn { turn },
    );
    let mut chat = Transcript::new("Codex");
    chat.delegation = Some(sheprd_chat_protocol::brief::DelegatedTask::new(
        Delegation {
            endpoint: parent.clone(),
            started_ms: 1000,
        },
        "iOS tests",
    ));
    chat.apply(&Change::Push {
        entry: Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                guest: None,
                text: "iOS tests".into(),
                files: 0,
                note: None,
            },
        },
    })
    .unwrap();

    live.change(&state, "child", &chat, &Change::Turn { turn });
    chat.turn = turn;
    live.snapshot(&state, "child", &chat);
    assert_eq!(live.groups.len(), 1);
    let ended = completion();
    live.change(&state, "child", &chat, &ended);
    let updates = live.pacer.due(now_ms() + 60_000);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].pane, "parent");
    let agents = updates[0].state.detail.agents.as_ref().unwrap();
    assert_eq!((agents.done, agents.total), (1, 1));
    assert_eq!(agents.latest.as_deref(), Some("Finished · iOS tests"));
}

fn completion() -> Change {
    Change::Push {
        entry: Entry {
            id: EntryId(2),
            kind: EntryKind::Turn(TurnSummary {
                ended_ms: Some(2000),
                duration_ms: 1000,
                end: TurnEnd::Done,
                tokens: None,
            }),
        },
    }
}

#[test]
fn a_reserved_worker_and_its_native_calls_outlive_the_coordinator_turn() {
    use sheprd_chat_protocol::{Activity, ActivityKind};
    let parent = EndpointId::random();
    let child = EndpointId::random();
    let owner = Delegation {
        endpoint: parent.clone(),
        started_ms: 1000,
    };
    let hub = Hub::offline(Layout {
        connected: true,
        workspaces: vec![
            workspace("parent", parent.clone()),
            workspace("child", child.clone()),
        ],
        ..Layout::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let mut state = crate::web::tests::state_with(dir.path(), hub);
    Arc::get_mut(&mut state).unwrap().apns = Delivery::new(true).unwrap();

    let mut live = LiveActivities::default();
    let mut root = Transcript::new("Codex");
    let turn = Some(Turn { started_ms: 1000 });
    live.change(&state, "parent", &root, &Change::Turn { turn });
    root.turn = turn;
    let mut worker = Activity {
        call_id: "call-1".into(),
        detail: None,
        finished_ms: Some(1300),
        kind: ActivityKind::Subagent,
        label: "Parent helper".into(),
        started_ms: 1050,
        state: ActivityState::Finished,
        stoppable: true,
    };
    live.change(
        &state,
        "parent",
        &root,
        &Change::Activity {
            activity: worker.clone(),
        },
    );
    // The desktop accepts the launch and the coordinator immediately returns.
    state.launches.report(
        owner.clone(),
        child.clone(),
        Some("Review".into()),
        now_ms(),
    );
    live.change(&state, "parent", &root, &completion());
    let agents = live.groups[&parent].update().state.detail.agents.unwrap();
    assert_eq!((agents.done, agents.total), (1, 2));
    // A tick before the child's pane is visible must keep the reservation.
    assert!(!live.groups.get_mut(&parent).unwrap().close_missing(
        &Layout {
            workspaces: vec![workspace("parent", parent.clone())],
            ..Layout::default()
        },
        now_ms()
    ));

    // The worker already finished its brief when first discovered, while its
    // own native agent (using the same local tool ID as the parent) continues.
    let mut chat = Transcript::new("Codex");
    chat.delegation = Some(sheprd_chat_protocol::brief::DelegatedTask::new(
        owner, "Review",
    ));
    chat.apply(&Change::Push {
        entry: Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                guest: None,
                text: "Review".into(),
                files: 0,
                note: None,
            },
        },
    })
    .unwrap();
    worker.finished_ms = None;
    worker.state = ActivityState::Running;
    worker.label = "Review helper".into();
    chat.apply(&Change::Activity {
        activity: worker.clone(),
    })
    .unwrap();
    chat.apply(&completion()).unwrap();
    live.snapshot(&state, "child", &chat);
    live.snapshot(&state, "child", &chat);
    assert_eq!(live.groups.len(), 1);
    let agents = live.groups[&parent].update().state.detail.agents.unwrap();
    assert_eq!((agents.done, agents.total), (2, 3));
    chat.apply(&Change::Push {
        entry: Entry {
            id: EntryId(3),
            kind: EntryKind::User {
                guest: None,
                text: "New task".into(),
                files: 0,
                note: None,
            },
        },
    })
    .unwrap();
    assert!(chat.delegated_brief().is_none());
    worker.state = ActivityState::Finished;
    live.change(
        &state,
        "child",
        &chat,
        &Change::Activity { activity: worker },
    );
    assert!(live.groups.is_empty());
}

#[test]
fn a_cancelled_or_unattached_launch_releases_the_parent() {
    let child = EndpointId::random();
    let owner = Delegation {
        endpoint: EndpointId::random(),
        started_ms: 500,
    };
    for cancel in [false, true] {
        let parent = live(Event::End { dismissal_s: 99 });
        let mut group = Group::new(owner.clone(), &parent.pane, parent.state.clone());
        let launches = Launches::default();
        launches.report(owner.clone(), child.clone(), Some("Worker".into()), 501);
        assert!(launches.deliver(&mut group, 502));
        assert!(matches!(group.parent(parent).event, Event::Update { .. }));
        assert!(!group.close_missing(&Layout::default(), 503));
        if cancel {
            launches.report(owner.clone(), child.clone(), None, 504);
            assert!(launches.deliver(&mut group, 505));
        } else {
            assert!(group.close_missing(&Layout::default(), u64::MAX));
        }
        assert!(matches!(group.update().event, Event::End { .. }));
    }
}

#[test]
fn cancellation_before_the_notifier_reads_the_launch_counts_one_failed_worker() {
    let owner = Delegation {
        endpoint: EndpointId::random(),
        started_ms: 10,
    };
    let child = EndpointId::random();
    let parent = live(Event::End { dismissal_s: 99 });
    let mut group = Group::new(owner.clone(), &parent.pane, parent.state.clone());
    let launches = Launches::default();
    launches.report(owner.clone(), child.clone(), Some("Parser".into()), 11);
    launches.report(owner, child, None, 12);
    assert!(launches.deliver(&mut group, 13));
    assert!(!launches.deliver(&mut group, 14));
    let final_state = group.parent(parent);
    assert!(matches!(final_state.event, Event::End { .. }));
    let agents = final_state.state.detail.agents.unwrap();
    assert_eq!((agents.done, agents.total), (1, 1));
    assert_eq!(agents.latest.as_deref(), Some("Failed · Parser"));
}
