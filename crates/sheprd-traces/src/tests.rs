#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    EventKind, Host, Root, Store, index,
    parse::{MAX_TEXT, Parser},
    redact::Redactor,
};
use sheprd_client::herdr::AgentKind;
use std::{fs, io::Write, path::Path};

mod fixtures;
mod overlap;
mod pi;
mod profile_roots;
mod scan_links;
mod structured_redaction;
mod watch;

const NOW: i64 = 1_800_000_000_000;

fn host() -> Host {
    Host {
        id: Host::id_of("test-machine"),
        label: "box".into(),
        os: "linux".into(),
        user: "me@box".into(),
    }
}

fn redactor() -> Redactor {
    Redactor::new(Some(Path::new("/home/me")))
}

fn roots(home: &Path) -> Vec<Root> {
    crate::default_roots(home)
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn append(path: &Path, text: &str) {
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

fn claude_file(home: &Path) -> std::path::PathBuf {
    home.join(".claude/projects/-home-me-code-app/0b5c7d1e.jsonl")
}

#[test]
fn claude_session_is_indexed_with_totals() {
    let home = tempfile::tempdir().unwrap();
    write(&claude_file(home.path()), fixtures::CLAUDE);
    let mut store = Store::in_memory().unwrap();
    let report = index(&mut store, &host(), &roots(home.path()), &redactor(), NOW).unwrap();
    assert_eq!((report.files, report.changed), (1, 1));

    let sessions = store.sessions(None, 10).unwrap();
    let [session] = sessions.as_slice() else {
        panic!("one session: {sessions:?}");
    };
    assert_eq!(session.agent, AgentKind::Claude);
    assert_eq!(session.native_id, "0b5c7d1e");
    assert_eq!(session.title.as_deref(), Some("Fix the login bug"));
    assert_eq!(session.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(session.cwd.as_deref(), Some("/home/me/code/app"));
    assert_eq!(session.branch.as_deref(), Some("fix/login"));
    assert_eq!(session.host, "box");
    // Two prompts and one reply; the subagent's record is not counted.
    assert_eq!((session.messages, session.user_messages), (3, 2));
    assert_eq!((session.tool_calls, session.errors), (1, 1));
    // Usage of the message split over two records counts once.
    assert_eq!(
        (session.tokens_in, session.tokens_out),
        (10 + 100 + 1000, 50)
    );
    assert!(session.started_at < session.updated_at);

    let events = store.events(&session.id, 0, 100).unwrap();
    let kinds: Vec<_> = events.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        [
            EventKind::User,
            EventKind::Thinking,
            EventKind::ToolCall,
            EventKind::Error,
            EventKind::Agent,
            EventKind::User,
        ]
    );
    assert_eq!(events[2].tool.as_deref(), Some("Bash"));
    assert!(events.windows(2).all(|pair| pair[0].seq < pair[1].seq));
}

#[test]
fn codex_rollout_is_indexed_without_injected_context() {
    let home = tempfile::tempdir().unwrap();
    let path = home
        .path()
        .join(".codex/sessions/2026/11/17/rollout-2026-11-17T10-00-00-abc.jsonl");
    write(&path, fixtures::CODEX);
    let mut store = Store::in_memory().unwrap();
    index(&mut store, &host(), &roots(home.path()), &redactor(), NOW).unwrap();

    let sessions = store.sessions(Some(AgentKind::Codex), 10).unwrap();
    let [session] = sessions.as_slice() else {
        panic!("one session: {sessions:?}");
    };
    assert_eq!(session.native_id, "019a-codex");
    assert_eq!(session.title.as_deref(), Some("Add a health check"));
    assert_eq!(session.model.as_deref(), Some("gpt-5.5-codex"));
    assert_eq!(session.agent_version.as_deref(), Some("0.90.0"));
    assert_eq!(session.branch.as_deref(), Some("main"));
    assert_eq!((session.tokens_in, session.tokens_out), (900, 120));
    assert_eq!((session.messages, session.tool_calls), (2, 1));
    let kinds: Vec<_> = store
        .events(&session.id, 0, 100)
        .unwrap()
        .iter()
        .map(|event| event.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::User,
            EventKind::Thinking,
            EventKind::ToolCall,
            EventKind::ToolResult,
            EventKind::Agent,
        ]
    );
    assert!(
        store
            .sessions(Some(AgentKind::Claude), 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn rescans_read_only_appended_complete_lines() {
    let home = tempfile::tempdir().unwrap();
    let path = claude_file(home.path());
    write(&path, fixtures::CLAUDE);
    let mut store = Store::in_memory().unwrap();
    let roots = roots(home.path());
    index(&mut store, &host(), &roots, &redactor(), NOW).unwrap();

    let unchanged = index(&mut store, &host(), &roots, &redactor(), NOW).unwrap();
    assert_eq!((unchanged.changed, unchanged.events), (0, 0));

    // A record still being written is not read until its newline lands.
    let (first, rest) = fixtures::CLAUDE_MORE.split_at(40);
    append(&path, first);
    let partial = index(&mut store, &host(), &roots, &redactor(), NOW).unwrap();
    assert_eq!(partial.events, 0);
    append(&path, rest);
    let finished = index(&mut store, &host(), &roots, &redactor(), NOW).unwrap();
    assert_eq!(finished.events, 1);
    let session = &store.sessions(None, 1).unwrap()[0];
    assert_eq!(session.messages, 4);
}

#[test]
fn replaced_file_is_read_again_from_the_start() {
    let home = tempfile::tempdir().unwrap();
    let path = claude_file(home.path());
    write(&path, fixtures::CLAUDE);
    let mut store = Store::in_memory().unwrap();
    let roots = roots(home.path());
    index(&mut store, &host(), &roots, &redactor(), NOW).unwrap();

    // Shorter than what was read: the old events must not linger.
    fs::remove_file(&path).unwrap();
    write(&path, fixtures::CLAUDE_MORE);
    index(&mut store, &host(), &roots, &redactor(), NOW).unwrap();
    let session = &store.sessions(None, 1).unwrap()[0];
    assert_eq!(session.messages, 1);
    assert_eq!(store.events(&session.id, 0, 100).unwrap().len(), 1);
}

#[test]
fn subagent_files_are_not_sessions() {
    let home = tempfile::tempdir().unwrap();
    write(&claude_file(home.path()), fixtures::CLAUDE);
    write(
        &home
            .path()
            .join(".claude/projects/-home-me-code-app/0b5c7d1e/subagents/agent-1.jsonl"),
        fixtures::CLAUDE_MORE,
    );
    let mut store = Store::in_memory().unwrap();
    let report = index(&mut store, &host(), &roots(home.path()), &redactor(), NOW).unwrap();
    assert_eq!(report.files, 1);
}

#[test]
fn secrets_and_home_are_redacted_before_they_are_kept() {
    let mut parser = Parser::new();
    let mut events = Vec::new();
    parser.record(
        AgentKind::Claude,
        0,
        fixtures::CLAUDE_SECRET,
        &redactor(),
        &mut events,
    );
    let [event] = events.as_slice() else {
        panic!("one event: {events:?}");
    };
    assert!(!event.text.contains("ghp_"), "{}", event.text);
    assert!(!event.text.contains("/home/me"), "{}", event.text);
    assert!(event.text.contains("~/code"), "{}", event.text);
    assert!(!parser.meta.title.unwrap_or_default().contains("ghp_"));
}

#[test]
fn long_text_is_bounded_on_a_character_boundary() {
    let long = "é".repeat(MAX_TEXT);
    let line = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": long},
    })
    .to_string();
    let mut events = Vec::new();
    Parser::new().record(AgentKind::Claude, 0, &line, &redactor(), &mut events);
    assert!(events[0].text.len() <= MAX_TEXT);
    assert!(events[0].text.chars().all(|c| c == 'é'));
}

#[test]
fn malformed_records_are_skipped() {
    let mut events = Vec::new();
    let mut parser = Parser::new();
    for line in ["", "{", "[]", "null", r#"{"type":"assistant","message":7}"#] {
        parser.record(AgentKind::Claude, 0, line, &redactor(), &mut events);
    }
    assert!(events.is_empty());
}

#[test]
fn activity_counts_messages_per_day_and_agent() {
    let home = tempfile::tempdir().unwrap();
    write(&claude_file(home.path()), fixtures::CLAUDE);
    let mut store = Store::in_memory().unwrap();
    index(&mut store, &host(), &roots(home.path()), &redactor(), NOW).unwrap();
    let activity = store.activity(0).unwrap();
    let [day] = activity.as_slice() else {
        panic!("one day: {activity:?}");
    };
    assert_eq!(day.day, "2026-11-17");
    assert_eq!(
        (day.agent, day.sessions, day.messages),
        (AgentKind::Claude, 1, 3)
    );
}

#[test]
fn the_same_machine_gets_the_same_id() {
    assert_eq!(Host::id_of("abc\n"), Host::id_of("abc"));
    assert_ne!(Host::id_of("abc"), Host::id_of("abd"));
}

#[test]
fn newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("traces").join(crate::FILE);
    drop(Store::open(&path).unwrap());
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", crate::SCHEMA_VERSION + 1)
        .unwrap();
    assert!(matches!(
        Store::open(&path),
        Err(crate::Error::NewerSchema { .. })
    ));
}
