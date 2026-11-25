use super::*;
use crate::apns::{ContentState, Phase};
use secrecy::ExposeSecret as _;

mod delegation;
mod host_cards;
mod relay;

fn live(event: Event) -> Live {
    Live {
        event,
        pane: "w1-p3".to_owned(),
        state: ContentState {
            detail: Default::default(),
            options: Vec::new(),
            permission: None,
            phase: Phase::Finished,
            plan_done: None,
            plan_total: None,
            previous: Vec::new(),
            question: None,
            started_at: 0,
            step: String::new(),
            summary: None,
            waiting: 0,
            working: 0,
        },
    }
}

fn registration(json: &str) -> Registration {
    Registration::parse(json.as_bytes()).unwrap()
}

mod detail {
    use super::*;
    use crate::app::config::ActivityDetail;

    const STEP: &str = "Editing secret.rs";

    fn question() -> Live {
        let mut live = live(Event::Update { alert: None });
        live.state.phase = Phase::Question;
        live.state.step = STEP.to_owned();
        live
    }

    #[test]
    fn a_phone_can_ask_for_less_than_its_computer_allows() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::web::tests::state(dir.path());
        let generic = registration(r#"{"activity_detail":"generic"}"#);
        assert_ne!(shown(&state, question(), &generic).state.step, STEP);
        let detailed = Registration::default();
        assert_eq!(shown(&state, question(), &detailed).state.step, STEP);
    }

    #[test]
    fn a_phone_cannot_ask_for_more_than_its_computer_allows() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = crate::web::tests::state(dir.path());
        Arc::get_mut(&mut state).unwrap().live_activities = ActivityDetail::Generic;
        let detailed = Registration::default();
        assert_ne!(shown(&state, question(), &detailed).state.step, STEP);
    }
}

#[test]
fn five_workers_share_the_parent_activity_through_its_last_completion() {
    use group::{Group, Member};
    let parent = Delegation {
        endpoint: EndpointId::random(),
        started_ms: 1000,
    };
    let mut root = live(Event::Update { alert: None });
    root.state.phase = Phase::Working;
    let mut group = Group::new(parent, &root.pane, root.state.clone());
    let mut workers = vec![
        Member::Chat(EndpointId::random()),
        Member::Chat(EndpointId::random()),
    ];
    workers.extend(
        ["parser", "ios", "review"]
            .map(|name| Member::Call(group.owner.endpoint.clone(), name.into())),
    );
    for member in &workers {
        assert!(group.record(member.clone(), ActivityState::Running, "Task"));
    }
    assert!(group.record(workers[2].clone(), ActivityState::Finished, "Parser"));
    assert!(!group.record(workers[2].clone(), ActivityState::Finished, "Parser"));
    let progress = group.parent(root.clone());
    assert!(matches!(progress.event, Event::Update { .. }));
    assert_eq!(progress.pane, root.pane);
    let agents = progress.state.detail.agents.unwrap();
    assert_eq!((agents.done, agents.total), (1, 5));
    assert_eq!(agents.latest.as_deref(), Some("Finished · Parser"));

    root.event = Event::End { dismissal_s: 99 };
    root.state.phase = Phase::Finished;
    let waiting = group.parent(root);
    assert_eq!(waiting.state.phase, Phase::Working);
    assert!(matches!(waiting.event, Event::Update { .. }));
    for index in [0, 1, 3, 4] {
        group.record(workers[index].clone(), ActivityState::Finished, "Reviewed");
    }
    let ended = group.update();
    assert!(matches!(ended.event, Event::End { .. }));
    assert_eq!(ended.state.detail.agents.unwrap().done, 5);
}

#[test]
fn a_follow_up_prompt_in_a_delegated_chat_is_independent() {
    use sheprd_chat_protocol::{Entry, EntryId};
    let mut chat = Transcript::new("Codex");
    chat.delegation = Some(sheprd_chat_protocol::brief::DelegatedTask::new(
        Delegation {
            endpoint: EndpointId::random(),
            started_ms: 12,
        },
        "Parser\nCheck tokenization",
    ));
    let prompt = |id, text: &str| Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
            note: None,
        },
    };
    chat.apply(&Change::Push {
        entry: prompt(1, "Parser\nCheck tokenization"),
    })
    .unwrap();

    assert_eq!(chat.delegated_brief().unwrap().1, "Parser");
    chat.apply(&Change::Truncate { before: EntryId(5) })
        .unwrap();
    assert_eq!(chat.delegated_brief().unwrap().1, "Parser");
    chat.apply(&Change::Push {
        entry: prompt(5, "Another job"),
    })
    .unwrap();
    assert!(chat.delegated_brief().is_none());
}

#[test]
fn only_subagents_from_the_delegating_turn_count() {
    use sheprd_chat_protocol::{Activity, ActivityKind, Turn};
    let mut transcript = Transcript::new("Codex");
    transcript.turn = Some(Turn { started_ms: 100 });
    let source = EndpointId::random();
    let mut group = group::seed(&source, "root", &transcript).unwrap();
    let mut activity = Activity {
        call_id: "tool-1".into(),
        detail: None,
        finished_ms: None,
        kind: ActivityKind::Subagent,
        label: "Parser".into(),
        started_ms: 90,
        state: ActivityState::Running,
        stoppable: true,
    };
    assert!(!group.activity(&source, &activity));
    activity.started_ms = 110;
    activity.kind = ActivityKind::Shell;
    assert!(!group.activity(&source, &activity));
    activity.kind = ActivityKind::Subagent;
    assert!(group.activity(&source, &activity));
    activity.detail = Some("Checking parser.rs".into());
    assert!(!group.activity(&source, &activity));
    activity.state = ActivityState::Failed;
    assert!(group.activity(&source, &activity));
    let agents = group.update().state.detail.agents.unwrap();
    assert_eq!((agents.done, agents.total), (1, 1));
    assert_eq!(agents.latest.as_deref(), Some("Failed · Parser"));
}
