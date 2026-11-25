use super::*;
use crate::apns::{AgentProgress, Alert, Attributes};

fn start(pane: &str) -> Live {
    let attributes = Attributes {
        agent: "Agent".into(),
        chat_title: pane.into(),
        computer: "studio".into(),
        computer_id: "AB".repeat(32),
        pane: pane.into(),
        workspace: "repo".into(),
        terminal: false,
    };
    let mut live = live(Event::Start {
        attributes,
        alert: Alert {
            title: "Started".into(),
            body: pane.into(),
        },
    });
    live.pane = pane.into();
    live.state.phase = Phase::Working;
    live
}

#[test]
fn two_phones_can_receive_one_or_multiple_cards_for_the_same_overlapping_turns() {
    let first = start("first");
    let mut second = start("second");
    let single = Registration {
        single_activity: true,
        ..Default::default()
    };
    let multiple = Registration::default();
    let mut joined = host::Cards::default();
    let mut separate = host::Cards::default();
    let sent = joined.offer(&first, &single, || None, 0).unwrap();
    let host_key = sent.pane.clone();
    assert!(matches!(sent.event, Event::Start { attributes, .. } if attributes.pane == host_key));
    assert_eq!(
        separate.offer(&first, &multiple, || None, 0),
        Some(first.clone())
    );
    second.state.phase = Phase::Question;
    second.state.permission = Some(7);
    second.state.detail.agents = Some(AgentProgress {
        done: 1,
        total: 4,
        latest: Some("Finished · tests".into()),
    });
    let asking = joined.offer(&second, &single, || None, 20).unwrap();
    assert!(matches!(asking.event, Event::Update { .. }));
    assert_eq!(asking.pane, host_key);
    assert_eq!(asking.state.permission, Some(7));
    assert_eq!(asking.state.detail.focus.as_ref().unwrap().pane, "second");
    let private = asking.clone().redacted(
        crate::push::Privacy::Generic,
        crate::app::config::ActivityDetail::Generic,
    );
    let private_focus = private.state.detail.focus.unwrap();
    assert!(private_focus.chat_title.is_empty());
    assert_eq!(private_focus.workspace, "studio");
    assert_eq!(private_focus.pane, "second");
    assert!(private.state.detail.agents.unwrap().latest.is_none());
    let progress = asking.state.detail.agents.unwrap();
    assert_eq!((progress.done, progress.total), (1, 5));
    assert_eq!(
        separate.offer(&second, &multiple, || None, 20),
        Some(second.clone())
    );
    let mut done = first;
    done.event = Event::End { dismissal_s: 5 };
    done.state.phase = Phase::Finished;
    // Changing the choice while these runs are active preserves their cards.
    let remaining = joined.offer(&done, &multiple, || None, 40).unwrap();
    assert_eq!(remaining.state.phase, Phase::Question);
    assert_eq!(remaining.state.detail.agents.unwrap().done, 2);
    assert_eq!(remaining.state.detail.focus.unwrap().pane, "second");
    assert_eq!(separate.offer(&done, &single, || None, 40), Some(done));
    second.event = Event::End { dismissal_s: 6 };
    second.state.phase = Phase::Finished;
    let end = joined.offer(&second, &multiple, || None, 60).unwrap();
    assert!(matches!(end.event, Event::End { .. }));
    assert_eq!(end.pane, host_key);
    assert_eq!(end.state.detail.agents.unwrap().done, 5);
    assert_eq!(
        joined
            .offer(&start("new"), &multiple, || None, 70)
            .unwrap()
            .pane,
        "new"
    );
}

#[test]
fn host_grouping_spans_worktrees_and_agent_grouping_uses_the_pane() {
    let reg = Registration {
        single_activity: true,
        ..Default::default()
    };
    let mut host = host::Cards::default();
    let mut agents = host::Cards::default();
    let first = start("first-agent");
    let card = host.offer(&first, &reg, || None, 0).unwrap();
    for (pane, workspace, now) in [
        ("second-agent", "repo", 15_000),
        ("third-agent", "other worktree", 30_000),
    ] {
        let mut next = start(pane);
        if let Event::Start { attributes, .. } = &mut next.event {
            workspace.clone_into(&mut attributes.workspace);
        }
        let combined = host.offer(&next, &reg, || None, now).unwrap();
        assert_eq!(combined.pane, card.pane);
        assert!(matches!(combined.event, Event::Update { .. }));
        assert_eq!(combined.state.detail.focus.unwrap().workspace, workspace);
        assert_eq!(
            agents.offer(&next, &Registration::default(), || None, now),
            Some(next)
        );
    }
}

#[test]
fn a_delayed_token_for_an_ended_host_card_cannot_swallow_the_next_start() {
    let mut cards = host::Cards::default();
    let mut reg = Registration {
        single_activity: true,
        ..Default::default()
    };
    let mut first = start("first");
    let initial = cards.offer(&first, &reg, || None, 0).unwrap();
    first.event = Event::End { dismissal_s: 20 };
    cards.offer(&first, &reg, || None, 1).unwrap();
    reg.activities = registration(
        &serde_json::json!({"activities":[{"pane":initial.pane,"token":"ab".repeat(32)}]})
            .to_string(),
    )
    .activities;
    cards.restore(&reg, || panic!("the previous card already ended"));
    let second = cards.offer(&start("first"), &reg, || None, 2).unwrap();
    assert!(matches!(second.event, Event::Start { .. }));
    assert_ne!(initial.pane, second.pane);
}

#[test]
fn restoring_a_shared_card_keeps_it_open_until_all_its_panes_finish() {
    let reg = registration(&serde_json::json!({"single_activity":true,"activities":[{"pane":"host-activity/restored","token":"ab".repeat(32)}]}).to_string());
    let mut cards = host::Cards::default();
    cards.restore(&reg, || {
        ["first", "second"]
            .into_iter()
            .map(start)
            .map(|live| {
                let Event::Start { attributes, .. } = live.event else {
                    panic!("expected start")
                };
                (attributes, live.state)
            })
            .collect()
    });
    let mut done = start("first");
    done.event = Event::End { dismissal_s: 10 };
    done.state.phase = Phase::Finished;
    let next = cards.offer(&done, &reg, || None, 20_000).unwrap();
    assert_eq!(next.pane, "host-activity/restored");
    assert!(matches!(next.event, Event::Update { .. }));
    assert_eq!(next.state.detail.focus.unwrap().pane, "second");
    done.pane = "second".into();
    assert!(matches!(
        cards.offer(&done, &reg, || None, 21_000).unwrap().event,
        Event::End { .. }
    ));
}

#[test]
fn a_phone_reset_replaces_the_host_card_even_before_its_first_token_arrives() {
    let mut cards = host::Cards::default();
    let mut reg = Registration {
        single_activity: true,
        ..Default::default()
    };
    let mut first = start("first");
    let original = cards.offer(&first, &reg, || None, 0).unwrap();
    // Ordinary registration lag never creates a duplicate start.
    assert!(cards.replace_reset(&reg, 5).is_none());
    reg.activity_epoch = Some(uuid::Uuid::new_v4());
    let (old, fresh) = cards.replace_reset(&reg, 6).unwrap();
    assert_eq!(old, original.pane);
    assert_ne!(old, fresh.pane);
    assert!(matches!(fresh.event, Event::Start { .. }));
    assert_eq!(fresh.state.detail.focus.unwrap().pane, "first");
    assert!(cards.replace_reset(&reg, 7).is_none());
    let joined = cards
        .offer(&start("second"), &reg, || None, 20_000)
        .unwrap();
    assert_eq!(joined.pane, fresh.pane);
    assert!(matches!(joined.event, Event::Update { .. }));
    assert_eq!(joined.state.detail.agents.unwrap().total, 2);

    first.event = Event::End { dismissal_s: 999 };
    cards.offer(&first, &reg, || None, 31_000).unwrap();
    let mut second = first;
    second.pane = "second".into();
    let end = cards.offer(&second, &reg, || None, 32_000).unwrap();
    assert_eq!(end.event, Event::End { dismissal_s: 999 });
    assert_eq!(end.state.detail.agents.unwrap().done, 2);
}

#[test]
fn the_last_run_controls_when_the_host_card_leaves_the_lock_screen() {
    for dismissal_s in [20, 920] {
        let reg = Registration {
            single_activity: true,
            ..Default::default()
        };
        let mut cards = host::Cards::default();
        let mut last = start("only");
        cards.offer(&last, &reg, || None, 0).unwrap();
        last.event = Event::End { dismissal_s };
        assert_eq!(
            cards.offer(&last, &reg, || None, 20_000).unwrap().event,
            last.event
        );
    }
}

#[test]
fn a_restart_with_only_the_relay_recipient_can_deliver_the_first_finish() {
    let reg = registration(&serde_json::json!({"single_activity":true,"activities":[{"pane":"relay/all-hosts","token":"ab".repeat(32)}]}).to_string());
    let mut cards = host::Cards::default();
    let mut done = start("pane");
    let Event::Start { attributes, .. } = &done.event else {
        unreachable!()
    };
    cards.restore(&reg, || vec![(attributes.clone(), done.state.clone())]);
    done.event = Event::End { dismissal_s: 20 };
    done.state.phase = Phase::Finished;
    let report = cards.offer(&done, &reg, || None, 10).unwrap();
    assert!(report.pane.starts_with("host-activity/restored-"));
    assert!(matches!(report.event, Event::End { .. }));
    assert_eq!(report.state.detail.agents.unwrap().done, 1);
}

#[test]
fn redacted_host_reports_distinguish_consecutive_questions_without_naming_them() {
    let reg = Registration {
        single_activity: true,
        ..Default::default()
    };
    let mut cards = host::Cards::default();
    let mut live = start("pane");
    live.state.phase = Phase::Question;
    live.state.permission = Some(42);
    let mut report = |live: &Live, now| {
        cards
            .offer(live, &reg, || None, now)
            .unwrap()
            .redacted(
                crate::push::Privacy::Generic,
                crate::app::config::ActivityDetail::Generic,
            )
            .state
    };
    let first = report(&live, 1);
    assert!(first.permission.is_none());
    let revision = first.detail.question_revision.unwrap();
    live.event = Event::Update { alert: None };
    assert_eq!(report(&live, 2).detail.question_revision, Some(revision));
    live.state.permission = Some(43);
    assert_ne!(report(&live, 3).detail.question_revision.unwrap(), revision);
}
