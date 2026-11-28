#![allow(clippy::unwrap_used, clippy::expect_used)]

use sheprd_chat_protocol::{
    Agent, Tool, ToolKind, ToolStatus, TurnEnd, TurnSummary,
    thread::{Thread, ThreadId},
};

use super::*;

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            guest: None,
            text: text.to_owned(),
            files: 0,
            note: None,
        },
    }
}

fn agent(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Agent {
            text: text.to_owned(),
        },
    }
}

fn tool(id: u64, title: &str, parent: Option<&str>) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: format!("c{id}"),
            content: Vec::new(),
            kind: ToolKind::Read,
            locations: Vec::new(),
            parent: parent.map(str::to_owned),
            started_ms: None,
            status: ToolStatus::Completed,
            title: title.to_owned(),
        }),
    }
}

fn turn(id: u64) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Turn(TurnSummary {
            duration_ms: 3_000,
            ended_ms: None,
            end: TurnEnd::Done,
            tokens: None,
        }),
    }
}

fn plain(entries: &[Entry]) -> Vec<String> {
    threaded(entries, None, "Claude")
}

fn threaded(entries: &[Entry], thread: Option<&Thread>, agent: &str) -> Vec<String> {
    lines(entries, thread, agent, 60, &mut HashMap::new())
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn each_author_gets_a_header_and_a_rail() {
    let text = plain(&[
        user(1, "fix the parser"),
        tool(2, "Read a.rs", None),
        tool(3, "Read b.rs", None),
        agent(4, "Done."),
        turn(5),
    ]);
    assert_eq!(text[0], " ◆ you");
    assert_eq!(text[1], " │ fix the parser");
    assert_eq!(text[2], " │ ");
    assert_eq!(text[3], " ◇ Claude");
    assert_eq!(text[4], " │ ✓ Read a.rs", "steps in a row stay together");
    assert_eq!(text[5], " │ ✓ Read b.rs");
    assert_eq!(text[6], " │ ");
    assert_eq!(text[7], " │   Done.");
    assert!(text[9].starts_with(" ╰─ Worked"), "{:?}", text[9]);
}

#[test]
fn a_new_prompt_after_a_turn_starts_another_pair_of_headers() {
    let text = plain(&[
        user(1, "one"),
        agent(2, "a"),
        turn(3),
        user(4, "two"),
        agent(5, "b"),
    ]);
    let headers: Vec<&str> = text
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with(" ◆") || line.starts_with(" ◇"))
        .collect();
    assert_eq!(headers, [" ◆ you", " ◇ Claude", " ◆ you", " ◇ Claude"]);
}

#[test]
fn a_sub_agents_steps_neither_draw_nor_split_steps() {
    let text = plain(&[
        tool(1, "Read a.rs", None),
        tool(2, "Read inner.rs", Some("task")),
        tool(3, "Read b.rs", None),
    ]);
    assert_eq!(text[1], " │ ✓ Read a.rs");
    assert_eq!(text[2], " │ ✓ Read b.rs");
    assert!(!text.iter().any(|line| line.contains("inner")));
}

#[test]
fn no_line_is_wider_than_asked() {
    let long = "word ".repeat(40);
    for width in [12, 20, 60] {
        let all = lines(
            &[user(1, &long), agent(2, &long), turn(3)],
            None,
            "Claude",
            width,
            &mut HashMap::new(),
        );
        for line in all {
            assert!(line.width() <= width, "{line:?} at {width}");
        }
    }
}

fn agent_headers(text: &[String]) -> Vec<&str> {
    text.iter()
        .map(String::as_str)
        .filter(|line| line.starts_with(" ◇"))
        .collect()
}

#[test]
fn replies_keep_the_agent_that_wrote_them_after_a_switch() {
    let mut thread = Thread::new(ThreadId::random(), Agent::Claude);
    thread.switch(Agent::Codex, Some(EntryId(3)));
    let entries = [
        user(1, "one"),
        agent(2, "a"),
        turn(3),
        user(4, "two"),
        agent(5, "b"),
    ];
    let text = threaded(&entries, Some(&thread), "Codex");
    assert_eq!(agent_headers(&text), [" ◇ Claude", " ◇ Codex"]);
}

#[test]
fn a_switch_inside_a_reply_names_the_new_agent() {
    let mut thread = Thread::new(ThreadId::random(), Agent::Claude);
    thread.switch(Agent::Codex, Some(EntryId(2)));
    let text = threaded(&[agent(2, "a"), agent(3, "b")], Some(&thread), "Codex");
    assert_eq!(agent_headers(&text), [" ◇ Claude", " ◇ Codex"]);
}
