#![allow(clippy::unwrap_used, clippy::expect_used)]

use sheprd_chat_protocol::{EntryId, ToolKind};

use super::*;

mod images;
mod steps;
mod tools;

fn plain(lines: &[Line<'_>]) -> Vec<String> {
    lines
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
fn wrapping_breaks_at_spaces_and_inside_long_words() {
    assert_eq!(wrap("hello world", 5), ["hello", "world"]);
    assert_eq!(wrap("hello  world", 8), ["hello", "world"]);
    assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
    assert_eq!(wrap("a\n\nb", 10), ["a", "", "b"]);
    assert_eq!(wrap("", 10), [""]);
    assert_eq!(wrap("  indented", 20), ["  indented"]);
}

#[test]
fn wrapping_counts_wide_characters_by_columns() {
    assert_eq!(wrap("日本語です", 4), ["日本", "語で", "す"]);
    for line in wrap("日本語 text 😀😀😀", 5) {
        let width: usize = line.chars().map(|ch| ch.width().unwrap_or(0)).sum();
        assert!(width <= 5, "{line}");
    }
}

#[test]
fn zero_width_still_makes_progress() {
    assert_eq!(wrap("ab", 0), ["a", "b"]);
}

#[test]
fn diff_counts_lines_added_and_removed() {
    assert_eq!(diff_counts("a\nb\nc", "a\nB\nc\nd"), (2, 1));
    assert_eq!(diff_counts("", "x\ny"), (2, 0));
    assert_eq!(diff_counts("x\nx", "x"), (0, 1));
}

#[test]
fn a_tool_entry_shows_status_title_and_diff_summary() {
    let item = Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(Tool {
            call_id: "c".to_owned(),
            content: vec![ToolContent::Diff {
                new: "a\nb".to_owned(),
                old: Some("a".to_owned()),
                path: "src/x.rs".to_owned(),
            }],
            kind: ToolKind::Edit,
            locations: vec!["src/x.rs:1".to_owned()],
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: "Edit x".to_owned(),
        }),
    };
    let lines = plain(&entry(&item, 40));
    assert_eq!(lines[0], "✓ Edit x");
    assert_eq!(lines[1], "  src/x.rs:1");
    assert_eq!(lines[2], "  src/x.rs +1 -0");
    assert_eq!(lines.last().map(String::as_str), Some(""));
}

#[test]
fn long_thoughts_fold() {
    let item = Entry {
        id: EntryId(1),
        kind: EntryKind::Thought {
            text: (0..10)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        },
    };
    let lines = plain(&entry(&item, 40));
    assert_eq!(lines.len(), THOUGHT_LINES + 2);
    assert_eq!(lines[THOUGHT_LINES], "  … 6 more lines");
}

#[test]
fn no_line_is_wider_than_asked() {
    let item = Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            guest: None,
            text: "a fairly long prompt that has to wrap more than once".to_owned(),
            files: 0,
            note: None,
        },
    };
    for line in plain(&entry(&item, 12)) {
        assert!(line.chars().count() <= 12, "{line}");
    }
}

#[test]
fn a_guests_prompt_is_named_above_its_words() {
    let item = Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            guest: Some(sheprd_chat_protocol::Guest::new("Bob", "tma")),
            text: "hi".to_owned(),
            files: 0,
            note: None,
        },
    };
    assert_eq!(plain(&entry(&item, 40)), ["Bob via tma · guest", "hi", ""]);
}

#[test]
fn effect_lines_are_left_out_but_their_text_stays() {
    let entry = Entry {
        id: EntryId(1),
        kind: EntryKind::Agent {
            text: ":::slam\nTests pass\n:::\n::confetti\n::not-an-effect\n```\n::confetti\n```"
                .to_owned(),
        },
    };
    let shown = plain(&super::entry(&entry, 40));
    let shown: Vec<&str> = shown.iter().map(|line| line.trim()).collect();
    assert!(shown.contains(&"Tests pass"), "{shown:?}");
    assert!(shown.contains(&"::not-an-effect"), "{shown:?}");
    assert!(
        !shown.iter().any(|line| line.starts_with(":::")),
        "{shown:?}"
    );
    // Code is shown as written, after its bar.
    assert_eq!(
        shown
            .iter()
            .filter(|line| line.ends_with("::confetti"))
            .count(),
        1
    );
}

#[test]
fn interaction_directives_keep_their_title_and_body_in_terminals() {
    let entry = Entry {
        id: EntryId(2),
        kind: EntryKind::Agent {
            text: ":::urgent Tap your YubiKey\nWaiting on git push\n:::\n::cue yubikey".to_owned(),
        },
    };
    let shown = plain(&super::entry(&entry, 40));
    let shown: Vec<&str> = shown.iter().map(|line| line.trim()).collect();
    assert!(shown.contains(&"Tap your YubiKey"), "{shown:?}");
    assert!(shown.contains(&"Waiting on git push"), "{shown:?}");
    assert!(
        !shown.iter().any(|line| line.starts_with("::")),
        "{shown:?}"
    );
}
