//! Which pull request a finished turn created or pushed to.

use crate::push::TurnPull;
use sheprd_chat_protocol::{
    Change, Entry, EntryId, EntryKind, PullRequest, PullRequestAction, Transcript, Turn, TurnEnd,
    TurnSummary,
};

fn pull(number: u64, actions: &[PullRequestAction]) -> Change {
    Change::PullRequest {
        pull_request: PullRequest::new("o/r", number, actions.to_vec()).unwrap(),
    }
}

fn push(kind: EntryKind) -> Change {
    Change::Push {
        entry: Entry {
            id: EntryId(10),
            kind,
        },
    }
}

fn user_kind() -> EntryKind {
    EntryKind::User {
        text: "go".to_owned(),
        files: 0,
        note: None,
    }
}

fn ended_kind() -> EntryKind {
    EntryKind::Turn(TurnSummary {
        ended_ms: None,
        duration_ms: 1_000,
        end: TurnEnd::Done,
        tokens: None,
    })
}

fn user() -> Change {
    push(user_kind())
}

fn ended() -> Change {
    push(ended_kind())
}

/// A transcript of `kinds`, numbered from 1.
fn transcript(kinds: Vec<EntryKind>) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    for (index, kind) in kinds.into_iter().enumerate() {
        transcript.entries.push(Entry {
            id: EntryId(index as u64 + 1),
            kind,
        });
    }
    transcript
}

#[test]
fn a_turn_ends_with_the_last_pull_request_it_created_or_pushed_to() {
    let mut turn = TurnPull::default();
    assert_eq!(turn.step(&user()), None);
    assert_eq!(turn.step(&pull(7, &[PullRequestAction::Pushed])), None);
    turn.step(&pull(8, &[PullRequestAction::Created]));
    // Read or commented on: no push, no button.
    turn.step(&pull(9, &[PullRequestAction::Commented]));
    turn.step(&pull(10, &[]));
    assert_eq!(
        turn.step(&ended()).as_deref(),
        Some("https://github.com/o/r/pull/8")
    );
    assert_eq!(turn.step(&ended()), None, "taken once");
}

#[test]
fn the_person_speaking_starts_a_turn_without_one() {
    let mut turn = TurnPull::default();
    turn.step(&pull(7, &[PullRequestAction::Pushed]));
    turn.step(&user());
    assert_eq!(turn.step(&ended()), None);
}

#[test]
fn a_fresh_copy_keeps_it_only_within_the_same_turn() {
    let touched = || {
        let mut turn = TurnPull::default();
        turn.step(&pull(7, &[PullRequestAction::Pushed]));
        turn
    };
    let running = |started_ms, kinds| Transcript {
        turn: Some(Turn { started_ms }),
        ..transcript(kinds)
    };
    let before = running(100, vec![user_kind()]);
    // The same turn still runs, even once a long transcript dropped the
    // entry that began it.
    for after in [
        running(100, vec![user_kind()]),
        running(
            100,
            vec![EntryKind::Agent {
                text: "pushed".to_owned(),
            }],
        ),
    ] {
        let mut kept = touched().carried(&before, &after);
        assert_eq!(
            kept.step(&ended()).as_deref(),
            Some("https://github.com/o/r/pull/7")
        );
    }
    // It ended, or another began, while the copy was away.
    for after in [
        transcript(vec![user_kind(), ended_kind()]),
        running(200, vec![user_kind(), ended_kind(), user_kind()]),
    ] {
        let mut dropped = touched().carried(&before, &after);
        assert_eq!(dropped.step(&ended()), None);
    }
}
