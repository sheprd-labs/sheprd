//! Keeping the chat's thread on disk: its record once a prompt reached an
//! agent, a segment per agent, and every entry in its transcript file.

use sheprd_chat_protocol::{
    EntryId,
    thread::{self, Record, Segment, Thread, ThreadId, file::Contents},
};

use crate::session::tests::threads::{handed_to_codex, respond_as, sent_prompt};
use crate::session::tests::*;

/// A chat that may switch between Claude and Codex, keeping thread `id` in
/// `dir`, whose agent opened session `s1`; `load` continues a session.
fn kept(dir: &std::path::Path, id: ThreadId, load: Option<&str>) -> Harness {
    kept_with(dir, id, load, None)
}

/// The chat's folder: absolute on every platform, as a record needs.
pub(super) fn work() -> PathBuf {
    std::env::temp_dir()
}

/// [`kept`], opening with `history` from the agent's own log.
fn kept_with(
    dir: &std::path::Path,
    id: ThreadId,
    load: Option<&str>,
    history: Option<Transcript>,
) -> Harness {
    let mut harness = harness_with(Config {
        choices: load.is_none().then(|| vec![Agent::Claude, Agent::Codex]),
        cwd: work(),
        history,
        load: load.map(str::to_owned),
        thread: Some(ThreadStart::read(dir.to_owned(), id).unwrap()),
        ..config()
    });
    harness.session.start();
    let initialize = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        initialize,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    match load {
        Some(_) => {
            let open = harness.last_request("session/load")["id"].as_i64().unwrap();
            // The session replays what the thread's file already holds.
            harness.agent_says("replayed");
            harness.respond(open, json!({}));
        }
        None => {
            let open = harness.last_request("session/new")["id"].as_i64().unwrap();
            harness.respond(open, json!({"sessionId": "s1"}));
        }
    }
    harness
}

fn record(dir: &std::path::Path, id: &ThreadId) -> Option<Record> {
    Record::read(&thread::record_path(dir, id)).ok()
}

fn contents(dir: &std::path::Path, id: &ThreadId) -> Contents {
    Contents::read(&thread::transcript_path(dir, id)).unwrap()
}

#[test]
fn a_thread_is_kept_once_a_prompt_reaches_its_agent() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut harness = kept(dir.path(), id, None);
    // Nothing is kept for a chat no one wrote in.
    assert!(record(dir.path(), &id).is_none());
    assert_eq!(
        harness.transcript().thread.as_ref().unwrap().segments[0]
            .session
            .as_deref(),
        Some("s1")
    );

    let turn = prompt(&mut harness, 1, "Fix the build\nplease");
    let kept = record(dir.path(), &id).expect("kept from the first prompt");
    assert_eq!(kept.title, "Fix the build");
    assert_eq!(kept.cwd, work());
    assert_eq!(
        kept.thread.segments,
        [Segment {
            after: None,
            agent: Agent::Claude,
            model: None,
            session: Some("s1".to_owned()),
        }]
    );
    harness.agent_says("Fixed");
    harness.respond(turn, json!({"stopReason": "end_turn"}));
    // The turn's end writes its entries, each at its latest.
    let written = contents(dir.path(), &id);
    assert!(written.entries.iter().any(|entry| matches!(
        &entry.kind,
        EntryKind::User { text, .. } if text.starts_with("Fix the build")
    )));
    assert!(written.entries.iter().any(|entry| matches!(
        &entry.kind,
        EntryKind::Agent { text } if text == "Fixed"
    )));
}

#[test]
fn a_switch_adds_a_segment_and_its_handoff_to_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut harness = kept(dir.path(), id, None);
    let codex = handed_to_codex(&mut harness);
    let thread = harness.transcript().thread.clone().unwrap();
    assert_eq!(record(dir.path(), &id).unwrap().thread, thread);
    let [claude, taking_over] = thread.segments.as_slice() else {
        panic!("{thread:?}");
    };
    assert_eq!(
        (claude.agent, claude.session.as_deref()),
        (Agent::Claude, Some("s1"))
    );
    assert_eq!(
        (taking_over.agent, taking_over.session.as_deref()),
        (Agent::Codex, Some("c1"))
    );
    // Codex began after the last entry Claude's turn left.
    let reply = harness
        .transcript()
        .entries
        .iter()
        .find(|entry| matches!(&entry.kind, EntryKind::Turn(_)))
        .unwrap()
        .id;
    assert_eq!(taking_over.after, Some(reply));

    let (turn, _) = sent_prompt(&mut harness, &codex, "next");
    respond_as(&mut harness, 1, turn, json!({"stopReason": "end_turn"}));
    let written = contents(dir.path(), &id);
    assert!(written.entries.iter().any(
        |entry| matches!(&entry.kind, EntryKind::Handoff(handoff) if handoff.to == Agent::Codex)
    ));
}

#[test]
fn ending_the_chat_writes_what_waits() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut harness = kept(dir.path(), id, None);
    prompt(&mut harness, 1, "hello");
    harness.agent_says("still streaming");
    assert_eq!(harness.input(Input::Shutdown), Flow::Exit);
    assert!(
        contents(dir.path(), &id)
            .entries
            .iter()
            .any(|entry| matches!(
                &entry.kind,
                EntryKind::Agent { text } if text == "still streaming"
            ))
    );
}

#[test]
fn a_continued_thread_goes_on_in_its_segment_after_its_entries() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut thread = Thread::new(id, Agent::Claude);
    thread.opened("s1");
    Record {
        created_ms: 1,
        cwd: work(),
        thread: thread.clone(),
        title: "Earlier".to_owned(),
        updated_ms: 1,
    }
    .write(dir.path())
    .unwrap();
    let mut writer = thread::file::Writer::open(dir.path(), &id).unwrap();
    let earlier = sheprd_chat_protocol::Entry {
        id: EntryId(40),
        kind: EntryKind::User {
            guest: None,
            text: "Earlier".to_owned(),
            files: 0,
            note: None,
        },
    };
    let replayed = sheprd_chat_protocol::Entry {
        id: EntryId(41),
        kind: EntryKind::Agent {
            text: "replayed".to_owned(),
        },
    };
    writer.append(&[&earlier, &replayed]).unwrap();

    let mut harness = kept(dir.path(), id, Some("s1"));
    assert_eq!(harness.transcript().thread.as_ref(), Some(&thread));
    prompt(&mut harness, 1, "again");
    assert_eq!(harness.input(Input::Shutdown), Flow::Exit);
    let written = contents(dir.path(), &id);
    // The replay is not written twice, and new entries follow the file's.
    assert_eq!(
        written
            .entries
            .iter()
            .filter(|entry| matches!(
                &entry.kind,
                EntryKind::Agent { text } if text == "replayed"
            ))
            .count(),
        1
    );
    let again = written
        .entries
        .iter()
        .find(|entry| matches!(&entry.kind, EntryKind::User { text, .. } if text == "again"))
        .unwrap();
    assert!(again.id > EntryId(40), "{again:?}");
    assert_eq!(record(dir.path(), &id).unwrap().title, "Earlier");
}

#[test]
fn a_continued_thread_with_another_session_starts_a_segment() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut thread = Thread::new(id, Agent::Codex);
    thread.opened("c1");
    Record {
        created_ms: 1,
        cwd: work(),
        thread,
        title: "Earlier".to_owned(),
        updated_ms: 1,
    }
    .write(dir.path())
    .unwrap();
    let harness = kept(dir.path(), id, Some("s1"));
    let segments = &harness.transcript().thread.as_ref().unwrap().segments;
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[1].agent, Agent::Claude);
}

#[test]
fn a_thread_that_cannot_be_written_says_so_once_and_stops() {
    let dir = tempfile::tempdir().unwrap();
    let threads = dir.path().join("threads");
    let id = ThreadId::random();
    let mut harness = kept(&threads, id, None);
    // A directory where the record should be prevents its atomic rename.
    std::fs::create_dir(thread::record_path(&threads, &id)).unwrap();
    let turn = prompt(&mut harness, 1, "hello");
    harness.respond(turn, json!({"stopReason": "end_turn"}));
    prompt(&mut harness, 1, "again");
    let warnings: Vec<_> = harness
        .notices()
        .into_iter()
        .filter(|(level, text)| {
            *level == NoticeLevel::Warning && text.contains("can no longer be kept")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn a_chat_without_a_thread_sends_none() {
    let harness = ready();
    assert_eq!(harness.transcript().thread, None);
}

fn user_entry(id: u64, text: &str) -> sheprd_chat_protocol::Entry {
    sheprd_chat_protocol::Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            guest: None,
            text: text.to_owned(),
            files: 0,
            note: None,
        },
    }
}

#[test]
fn a_new_thread_keeps_the_conversation_a_resumed_session_opened_with() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut history = Transcript::new("Claude");
    history.entries = vec![user_entry(7, "From the agent's log")];
    let mut harness = kept_with(dir.path(), id, Some("s1"), Some(history));
    // Kept at once: the conversation already started.
    let kept = record(dir.path(), &id).expect("kept as the session opened");
    assert_eq!(kept.title, "From the agent's log");
    assert_eq!(harness.input(Input::Shutdown), Flow::Exit);
    let written = contents(dir.path(), &id);
    assert_eq!(written.entries[0], user_entry(7, "From the agent's log"));
    // What the session replayed is the conversation too.
    assert!(written.entries.iter().any(|entry| matches!(
        &entry.kind,
        EntryKind::Agent { text } if text == "replayed"
    )));
}

#[test]
fn a_thread_kept_by_a_newer_program_is_not_written_over() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let path = thread::record_path(dir.path(), &id);
    std::fs::write(&path, br#"{"version":99}"#).unwrap();
    assert!(matches!(
        ThreadStart::read(dir.path().to_owned(), id),
        Err(thread::Error::Json(_) | thread::Error::Version(_))
    ));
}

#[test]
fn a_late_update_to_an_entry_the_window_then_drops_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut harness = kept(dir.path(), id, None);
    prompt(&mut harness, 1, "hello");
    let call = |id: &str, text: &str, status: &str| {
        json!({"sessionUpdate": "tool_call", "toolCallId": id, "title": id, "kind": "execute",
            "status": status, "content": [{"type": "content", "content": {"type": "text", "text": text}}]})
    };
    harness.update(call("first", "", "pending"));
    // Enough small calls to write what waits, "first" still pending.
    for at in 0..64 {
        harness.update(call(&format!("small-{at}"), "", "completed"));
    }
    assert!(
        contents(dir.path(), &id)
            .entries
            .iter()
            .any(|entry| matches!(
                &entry.kind,
                EntryKind::Tool(tool) if tool.call_id == "first"
            ))
    );
    let big = "x".repeat(200 * 1024);
    for at in 0..9 {
        harness.update(call(&format!("big-{at}"), &big, "completed"));
    }
    // "first" ends with output that takes the window past its bound, so the
    // window drops it as it changes.
    harness.update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "first",
        "status": "completed", "content": [{"type": "content",
        "content": {"type": "text", "text": "y".repeat(250 * 1024)}}]}),
    );
    assert!(
        !harness.transcript().entries.iter().any(|entry| matches!(
            &entry.kind,
            EntryKind::Tool(tool) if tool.call_id == "first"
        )),
        "the window still holds it"
    );
    let written = contents(dir.path(), &id);
    let first = written
        .entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Tool(tool) if tool.call_id == "first" => Some(tool),
            _ => None,
        })
        .unwrap();
    assert_eq!(first.status, sheprd_chat_protocol::ToolStatus::Completed);
}

#[test]
fn a_crashed_turn_is_recovered_from_history_without_overwriting_saved_ids() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut first = kept(dir.path(), id, None);
    let turn = prompt(&mut first, 1, "saved");
    first.respond(turn, json!({"stopReason": "end_turn"}));
    prompt(&mut first, 2, "not flushed");
    let before = contents(dir.path(), &id);
    drop(first); // No Shutdown: model a killed chat.

    let mut history = Transcript::new("Claude");
    // IDs in the agent log are unrelated to IDs allocated by the chat.
    history.entries = vec![user_entry(1, "saved"), user_entry(2, "not flushed")];
    let mut resumed = kept_with(dir.path(), id, Some("s1"), Some(history.clone()));
    resumed.input(Input::Shutdown);
    let recovered = contents(dir.path(), &id);
    for entry in &before.entries {
        assert!(recovered.entries.contains(entry));
    }
    assert_eq!(
        recovered
            .entries
            .iter()
            .filter(|entry| matches!(
                &entry.kind, EntryKind::User { text, .. } if text == "not flushed"
            ))
            .count(),
        1
    );
    drop(resumed);
    let mut again = kept_with(dir.path(), id, Some("s1"), Some(history));
    again.input(Input::Shutdown);
    let messages = |contents: Contents| {
        contents
            .entries
            .into_iter()
            .filter(|entry| !matches!(entry.kind, EntryKind::Notice { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(messages(contents(dir.path(), &id)), messages(recovered));
}

#[test]
fn a_record_without_flushed_entries_recovers_the_load_replay() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut first = kept(dir.path(), id, None);
    prompt(&mut first, 1, "interrupted");
    assert!(contents(dir.path(), &id).entries.is_empty());
    drop(first);
    let mut resumed = kept(dir.path(), id, Some("s1"));
    resumed.input(Input::Shutdown);
    assert!(
        contents(dir.path(), &id)
            .entries
            .iter()
            .any(|entry| matches!(
                &entry.kind, EntryKind::Agent { text } if text == "replayed"
            ))
    );
}

#[test]
fn a_thread_has_one_owner_until_the_chat_releases_it() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let first = kept(dir.path(), id, None);
    assert!(matches!(
        ThreadStart::read(dir.path().to_owned(), id),
        Err(thread::Error::Locked)
    ));
    // Another thread is independent, and dropping its start releases it.
    let other = ThreadId::random();
    drop(ThreadStart::read(dir.path().to_owned(), other).unwrap());
    drop(ThreadStart::read(dir.path().to_owned(), other).unwrap());
    drop(first);
    drop(ThreadStart::read(dir.path().to_owned(), id).unwrap());
}

#[test]
fn restored_tools_keep_their_identity_timing_and_conversation_order() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut first = kept(dir.path(), id, None);
    let turn = prompt(&mut first, 1, "run it");
    first.update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "command-1",
        "title": "command", "kind": "execute", "status": "in_progress",
        "content": [{"type": "diff", "path": "/work/file", "oldText": "before", "newText": "after"}]
    }));
    first.update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "command-1", "status": "completed"
    }));
    first.agent_says("finished");
    first.respond(turn, json!({"stopReason": "end_turn"}));
    let saved = contents(dir.path(), &id);
    let tool = saved
        .entries
        .iter()
        .find(|entry| matches!(entry.kind, EntryKind::Tool(_)))
        .unwrap()
        .clone();
    let EntryKind::Tool(saved_tool) = &tool.kind else {
        panic!("tool")
    };
    assert!(saved_tool.started_ms.is_some());
    drop(first);

    let mut history = Transcript::new("Claude");
    history.entries = saved
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                EntryKind::User { .. } | EntryKind::Tool(_) | EntryKind::Agent { .. }
            )
        })
        .cloned()
        .collect();
    for (at, entry) in history.entries.iter_mut().enumerate() {
        entry.id = EntryId(at as u64 + 1);
        if let EntryKind::Tool(tool) = &mut entry.kind {
            tool.started_ms = None;
            tool.content = vec![sheprd_chat_protocol::ToolContent::Text {
                text: "Recovered output".to_owned(),
            }];
        }
    }
    // Recovery may also contain a missing entry between two saved entries.
    history.entries.insert(2, user_entry(50, "missing middle"));
    let expected: Vec<_> = history
        .entries
        .iter()
        .map(|entry| entry.kind.clone())
        .collect();
    for _ in 0..2 {
        let mut resumed = kept_with(dir.path(), id, Some("s1"), Some(history.clone()));
        let shown = &resumed.transcript().entries;
        assert_eq!(
            shown
                .iter()
                .take(expected.len())
                .map(|entry| entry.kind.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(shown.windows(2).all(|pair| pair[0].id < pair[1].id));
        resumed.input(Input::Shutdown);
        let kept = contents(dir.path(), &id);
        let tools: Vec<_> = kept
            .entries
            .iter()
            .filter(
                |entry| matches!(&entry.kind, EntryKind::Tool(tool) if tool.call_id == "command-1"),
            )
            .collect();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, tool.id);
        let EntryKind::Tool(restored) = &tools[0].kind else {
            panic!("tool")
        };
        assert_eq!(restored.started_ms, saved_tool.started_ms);
        assert!(
            saved_tool
                .content
                .iter()
                .any(|content| matches!(content, sheprd_chat_protocol::ToolContent::Diff { .. }))
        );
        for content in &saved_tool.content {
            assert!(restored.content.contains(content));
        }
        assert_eq!(restored.content.iter().filter(|content| matches!(
            content, sheprd_chat_protocol::ToolContent::Text { text } if text == "Recovered output"
        )).count(), 1);
    }
}
