#![allow(clippy::unwrap_used, clippy::expect_used)]

use sheprd_chat_protocol::{
    Activity, ActivityKind, ActivityState, Cursor, PermissionId, Status, Transcript,
};

use super::*;

fn activity() -> Change {
    Change::Activity {
        activity: Activity {
            call_id: "task".to_owned(),
            detail: None,
            finished_ms: None,
            kind: ActivityKind::Subagent,
            label: "Review the reducer tests".to_owned(),
            started_ms: 1_000,
            state: ActivityState::Running,
            stoppable: false,
        },
    }
}

fn change(change: Change, seq: u64, epoch: Cursor) -> ServerMessage {
    ServerMessage::Change {
        change,
        cursor: Cursor { seq, ..epoch },
    }
}

fn snapshot(clients: u32, cursor: Cursor) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.clients = clients;
    ServerMessage::Snapshot {
        cursor,
        transcript: Box::new(transcript),
    }
}

#[test]
fn a_current_client_gets_every_message_unchanged() {
    let mut downgrade = Downgrade::new(REVISION);
    let epoch = Cursor::new_epoch();
    let message = change(activity(), 1, epoch);
    assert_eq!(downgrade.message(message.clone()), Some(message));
}

#[test]
fn an_older_client_gets_the_client_count_it_holds_at_the_same_cursor() {
    let mut downgrade = Downgrade::new(0);
    let epoch = Cursor::new_epoch();
    assert!(downgrade.message(snapshot(2, epoch)).is_some());
    assert_eq!(
        downgrade.message(change(activity(), 1, epoch)),
        Some(change(Change::Clients { count: 2 }, 1, epoch))
    );
    let count = change(Change::Clients { count: 3 }, 2, epoch);
    assert_eq!(downgrade.message(count.clone()), Some(count));
    assert_eq!(
        downgrade.message(change(activity(), 3, epoch)),
        Some(change(Change::Clients { count: 3 }, 3, epoch)),
        "the count follows the changes the client applied"
    );
}

#[test]
fn an_older_client_gets_changes_it_knows_unchanged() {
    let mut downgrade = Downgrade::new(0);
    let epoch = Cursor::new_epoch();
    downgrade.message(snapshot(1, epoch));
    for known in [
        Change::Status {
            status: Status::Working,
        },
        Change::PermissionResolved {
            id: PermissionId(4),
        },
    ] {
        let message = change(known, 1, epoch);
        assert_eq!(downgrade.message(message.clone()), Some(message));
    }
}

#[test]
fn an_older_client_without_a_snapshot_is_dropped_rather_than_guessed_at() {
    let mut downgrade = Downgrade::new(0);
    let epoch = Cursor::new_epoch();
    assert_eq!(downgrade.message(change(activity(), 1, epoch)), None);
}

#[test]
fn a_client_before_harnesses_gets_the_count_in_their_place() {
    let harnesses = Change::Harnesses { harnesses: None };
    let epoch = Cursor::new_epoch();
    let mut revision_one = Downgrade::new(1);
    assert!(revision_one.message(snapshot(1, epoch)).is_some());
    // Revision 1 reads activities but not the choice of agents.
    let activity = change(activity(), 1, epoch);
    assert_eq!(revision_one.message(activity.clone()), Some(activity));
    assert_eq!(
        revision_one.message(change(harnesses.clone(), 2, epoch)),
        Some(change(Change::Clients { count: 1 }, 2, epoch))
    );
    let mut current = Downgrade::new(REVISION);
    let message = change(harnesses, 1, epoch);
    assert_eq!(current.message(message.clone()), Some(message));
}

#[test]
fn a_client_before_pull_requests_gets_the_count_in_their_place() {
    let pull = Change::PullRequest {
        pull_request: sheprd_chat_protocol::PullRequest::parse("https://github.com/o/r/pull/1")
            .unwrap(),
    };
    let epoch = Cursor::new_epoch();
    let mut revision_two = Downgrade::new(2);
    assert!(revision_two.message(snapshot(1, epoch)).is_some());
    let harnesses = change(Change::Harnesses { harnesses: None }, 1, epoch);
    assert_eq!(revision_two.message(harnesses.clone()), Some(harnesses));
    assert_eq!(
        revision_two.message(change(pull.clone(), 2, epoch)),
        Some(change(Change::Clients { count: 1 }, 2, epoch))
    );
    let mut current = Downgrade::new(REVISION);
    let message = change(pull, 1, epoch);
    assert_eq!(current.message(message.clone()), Some(message));
}

mod briefs;

fn handoff_entry() -> Entry {
    Entry {
        id: sheprd_chat_protocol::EntryId(4),
        kind: EntryKind::Handoff(sheprd_chat_protocol::HandedOver {
            from: Some(sheprd_chat_protocol::Agent::Claude),
            kept: 3,
            omitted: 0,
            resumed: false,
            to: sheprd_chat_protocol::Agent::Codex,
        }),
    }
}

#[test]
fn a_client_before_threads_gets_handoffs_as_notices_and_no_thread() {
    use sheprd_chat_protocol::{
        Agent,
        thread::{Thread, ThreadId},
    };
    let mut downgrade = Downgrade::new(4);
    let epoch = Cursor::new_epoch();
    let thread = Thread::new(ThreadId::random(), Agent::Claude);
    let mut transcript = Transcript::new("Codex");
    transcript.clients = 1;
    transcript.entries.push(handoff_entry());
    transcript.thread = Some(thread.clone());
    let Some(ServerMessage::Snapshot { transcript, .. }) =
        downgrade.message(ServerMessage::Snapshot {
            cursor: epoch,
            transcript: Box::new(transcript),
        })
    else {
        panic!("no snapshot");
    };
    assert_eq!(transcript.thread, None);
    let EntryKind::Notice { text, .. } = &transcript.entries[0].kind else {
        panic!("not a notice");
    };
    assert!(text.starts_with("Codex continues this conversation"));

    let Some(ServerMessage::Change {
        change: Change::Push { entry },
        ..
    }) = downgrade.message(change(
        Change::Push {
            entry: handoff_entry(),
        },
        1,
        epoch,
    ))
    else {
        panic!("no push");
    };
    assert!(matches!(entry.kind, EntryKind::Notice { .. }));
    assert_eq!(
        downgrade.message(change(Change::Thread { thread }, 2, epoch)),
        Some(change(Change::Clients { count: 1 }, 2, epoch))
    );
}

#[test]
fn a_client_of_threads_gets_handoffs_as_they_are() {
    let mut downgrade = Downgrade::new(REVISION);
    let epoch = Cursor::new_epoch();
    let push = change(
        Change::Push {
            entry: handoff_entry(),
        },
        1,
        epoch,
    );
    assert_eq!(downgrade.message(push.clone()), Some(push));
}

#[test]
fn an_older_client_reads_a_guests_words_named_in_their_text() {
    let guest = sheprd_chat_protocol::Guest::new("Bob", "tma");
    let entry = Entry {
        id: sheprd_chat_protocol::EntryId(4),
        kind: EntryKind::User {
            guest: Some(guest.clone()),
            text: "hi".to_owned(),
            files: 0,
            note: None,
        },
    };
    let mut transcript = Transcript::new("Claude");
    transcript.entries.push(entry.clone());
    let epoch = Cursor::new_epoch();
    let mut downgrade = Downgrade::new(5);
    let Some(ServerMessage::Snapshot { transcript, .. }) =
        downgrade.message(ServerMessage::Snapshot {
            cursor: epoch,
            transcript: Box::new(transcript),
        })
    else {
        panic!("no snapshot");
    };
    let named = EntryKind::User {
        guest: None,
        text: "Bob via tma: hi".to_owned(),
        files: 0,
        note: None,
    };
    assert_eq!(transcript.entries[0].kind, named);
    let Some(ServerMessage::Change {
        change: Change::Push { entry: pushed },
        ..
    }) = downgrade.message(change(
        Change::Push {
            entry: entry.clone(),
        },
        1,
        epoch,
    ))
    else {
        panic!("no push");
    };
    assert_eq!(pushed.kind, named);
    // A current client gets the guest as it is.
    let mut current = Downgrade::new(REVISION);
    let push = change(Change::Push { entry }, 1, epoch);
    assert_eq!(current.message(push.clone()), Some(push));
}
