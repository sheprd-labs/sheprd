use serde_json::json;

use crate::{
    Agent, Change, Entry, EntryId, EntryKind, HandedOver, Transcript,
    thread::{MAX_SEGMENTS, Segment, Thread, ThreadId},
};

fn handoff() -> HandedOver {
    HandedOver {
        from: Some(Agent::Claude),
        kept: 42,
        omitted: 17,
        resumed: false,
        to: Agent::Codex,
    }
}

#[test]
fn a_handoff_entry_reads_by_name() {
    let entry = Entry {
        id: EntryId(3),
        kind: EntryKind::Handoff(handoff()),
    };
    let value = serde_json::to_value(&entry).unwrap();
    assert_eq!(
        value,
        json!({"id": 3, "kind": {"type": "handoff", "from": "claude", "kept": 42, "omitted": 17, "to": "codex"}})
    );
    assert_eq!(serde_json::from_value::<Entry>(value).unwrap(), entry);
    assert_eq!(entry.kind.text(), None);
}

#[test]
fn a_handoff_reads_as_a_notice_for_older_clients() {
    let EntryKind::Notice { text, .. } = handoff().as_notice() else {
        panic!("not a notice");
    };
    assert_eq!(
        text,
        "Codex continues this conversation and gets the conversation so far with your next \
         prompt: 42 entries, 17 more left out for length."
    );
    let resumed = HandedOver {
        omitted: 0,
        resumed: true,
        ..handoff()
    };
    assert_eq!(
        resumed.notice_text(),
        "Codex continues this conversation and gets what happened since it last took part with \
         your next prompt."
    );
}

#[test]
fn a_thread_change_replaces_the_thread_and_refuses_one_out_of_bounds() {
    let mut transcript = Transcript::default();
    let mut thread = Thread::new(ThreadId::random(), Agent::Claude);
    transcript
        .apply(&Change::Thread {
            thread: thread.clone(),
        })
        .unwrap();
    assert_eq!(transcript.thread.as_ref(), Some(&thread));
    thread.segments = vec![Segment::new(Agent::Codex, None); MAX_SEGMENTS + 1];
    assert!(transcript.apply(&Change::Thread { thread }).is_err());
    assert_eq!(transcript.thread.as_ref().unwrap().segments.len(), 1);
}

#[test]
fn a_transcript_without_a_thread_leaves_the_field_out() {
    let value = serde_json::to_value(Transcript::default()).unwrap();
    assert!(value.get("thread").is_none());
}
