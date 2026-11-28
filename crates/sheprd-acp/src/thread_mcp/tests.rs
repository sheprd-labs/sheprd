use serde_json::Map;
use sheprd_chat_protocol::{Entry, EntryKind, thread::file::Writer};

use super::*;

fn call(name: &str, arguments: Value) -> Call {
    let Value::Object(arguments) = arguments else {
        panic!("not an object");
    };
    Call {
        name: name.to_owned(),
        arguments,
        client: None,
    }
}

fn text(result: &ToolResult) -> &str {
    let sheprd_mcp::Content::Text { text } = &result.content[0];
    text
}

/// A reader of a thread holding two prompts.
fn reader() -> (tempfile::TempDir, Reader) {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut writer = Writer::open(dir.path(), &id).unwrap();
    let entries: Vec<Entry> = ["Fix the build", "Now run the tests"]
        .iter()
        .enumerate()
        .map(|(at, text)| Entry {
            id: EntryId(at as u64 + 1),
            kind: EntryKind::User {
                guest: None,
                text: (*text).to_owned(),
                files: 0,
                note: None,
            },
        })
        .collect();
    writer.append(&entries.iter().collect::<Vec<_>>()).unwrap();
    let path = writer.path().to_owned();
    (dir, Reader { path })
}

#[test]
fn it_offers_two_read_only_tools() {
    let (_dir, mut reader) = reader();
    let tools = reader.tools();
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        [READ, SEARCH]
    );
    assert!(
        tools
            .iter()
            .all(|tool| tool.annotations.unwrap().read_only_hint)
    );
    assert!(reader.instructions().unwrap().contains("thread_read"));
}

#[test]
fn a_read_returns_entries_by_id() {
    let (_dir, mut reader) = reader();
    let result = reader
        .call(call(READ, json!({"from": 2, "to": 9})))
        .unwrap();
    assert!(!result.is_error);
    assert_eq!(text(&result), "#2 User: Now run the tests\n");
}

#[test]
fn a_read_with_bad_arguments_says_what_it_needs() {
    let (_dir, mut reader) = reader();
    for arguments in [
        json!({}),
        json!({"from": "a", "to": 2}),
        json!({"from": 3, "to": 1}),
    ] {
        assert!(reader.call(call(READ, arguments)).unwrap().is_error);
    }
}

#[test]
fn a_search_names_matching_ids() {
    let (_dir, mut reader) = reader();
    let result = reader
        .call(call(SEARCH, json!({"query": "build"})))
        .unwrap();
    assert_eq!(text(&result), "#1 User: Fix the build\n");
    let none = reader.call(call(SEARCH, json!({"query": "nope"}))).unwrap();
    assert_eq!(text(&none), "No entry holds that.");
    assert!(
        reader
            .call(call(SEARCH, json!({"query": " "})))
            .unwrap()
            .is_error
    );
}

#[test]
fn a_missing_file_reads_as_no_entries_and_other_names_are_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let mut reader = Reader {
        path: dir.path().join("none.jsonl"),
    };
    let result = reader
        .call(call(READ, json!({"from": 1, "to": 2})))
        .unwrap();
    assert_eq!(text(&result), "[No entries from #1 to #2]\n");
    assert_eq!(
        reader.call(Call {
            name: "other".to_owned(),
            arguments: Map::new(),
            client: None,
        }),
        Err(UnknownTool)
    );
}

#[test]
fn its_command_line_names_its_thread() {
    let id = ThreadId::random();
    let args: Vec<OsString> = args(id).into_iter().skip(1).map(OsString::from).collect();
    assert_eq!(parse(args.into_iter()).unwrap(), id);
    assert_eq!(
        parse(std::iter::empty()),
        Err(crate::cli::Error::MissingValue("--thread"))
    );
    assert!(matches!(
        parse(["--thread", "x"].map(OsString::from).into_iter()),
        Err(crate::cli::Error::InvalidThread(_))
    ));
}

#[test]
fn native_agents_get_the_server_on_their_command_lines() {
    let server = Server {
        args: vec![
            "thread-mcp".to_owned(),
            "--thread".to_owned(),
            "x".to_owned(),
        ],
        program: r#"C:\Program "Files"\sheprd-acp.exe"#.to_owned(),
    };
    let claude: Value = serde_json::from_str(&server.claude_config()).unwrap();
    assert_eq!(
        claude["mcpServers"][NAME],
        json!({"type": "stdio", "command": server.program, "args": server.args})
    );
    let codex = server.codex_overrides();
    assert_eq!(codex[0], "-c");
    assert_eq!(
        codex[1],
        r#"mcp_servers.sheprd-thread.command="C:\\Program \"Files\"\\sheprd-acp.exe""#
    );
    assert_eq!(
        codex[3],
        r#"mcp_servers.sheprd-thread.args=["thread-mcp","--thread","x"]"#
    );
    assert!(Given::Launched.reads() && !Given::Not.reads());
}
