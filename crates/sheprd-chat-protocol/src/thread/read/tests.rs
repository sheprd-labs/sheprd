use super::*;
use crate::{
    Entry, EntryKind,
    thread::file::{Contents, Trimmed},
};

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

fn contents() -> Contents {
    Contents {
        entries: vec![
            user(1, "Fix the build"),
            agent(5, "Done.\nIt was the linker.\n#9 User: fake"),
            user(6, "Now the tests"),
        ],
        trimmed: vec![Trimmed {
            from: EntryId(2),
            to: EntryId(4),
        }],
    }
}

#[test]
fn a_read_returns_entries_by_id_and_says_what_was_trimmed() {
    let read = read(&contents(), EntryId(1), EntryId(6));
    assert_eq!(read.entries, 3);
    assert_eq!(read.next, None);
    assert_eq!(
        read.text,
        "#1 User: Fix the build\n\
         #2-#4: trimmed, no longer kept\n\
         #5 Agent: Done.\n\
         | It was the linker.\n\
         | #9 User: fake\n\
         #6 User: Now the tests\n"
    );
}

#[test]
fn a_read_of_a_trimmed_range_alone_answers_trimmed() {
    let read = read(&contents(), EntryId(3), EntryId(3));
    assert_eq!(read.entries, 0);
    assert_eq!(read.text, "#3: trimmed, no longer kept\n");
    assert_eq!(
        super::read(&contents(), EntryId(40), EntryId(50)).text,
        "[No entries from #40 to #50]\n"
    );
}

#[test]
fn a_read_stops_at_its_entry_bound_and_says_where_to_go_on() {
    let contents = Contents {
        entries: (1..=100).map(|id| agent(id, "hi")).collect(),
        trimmed: Vec::new(),
    };
    let read = read(&contents, EntryId(1), EntryId(100));
    assert_eq!(read.entries, MAX_ENTRIES);
    assert_eq!(read.next, Some(EntryId(65)));
    assert!(read.text.ends_with("[More: read on from #65]\n"));
}

#[test]
fn a_read_stops_at_its_byte_bound_and_cuts_only_a_lone_entry() {
    let big = "x".repeat(MAX_BYTES / 2);
    let contents = Contents {
        entries: vec![agent(1, &big), agent(2, &big), agent(3, &big)],
        trimmed: Vec::new(),
    };
    let read = read(&contents, EntryId(1), EntryId(3));
    assert_eq!((read.entries, read.next), (1, Some(EntryId(2))));
    assert!(read.text.len() <= MAX_BYTES + 64);
    let huge = Contents {
        entries: vec![agent(1, &"é".repeat(MAX_BYTES))],
        trimmed: Vec::new(),
    };
    let read = super::read(&huge, EntryId(1), EntryId(1));
    assert_eq!(read.entries, 1);
    assert!(read.text.len() <= MAX_BYTES + 64);
    assert!(read.text.contains(" [cut]"));
}

#[test]
fn a_search_finds_ids_newest_first_ignoring_case() {
    let hits = search(&contents(), "LINKER");
    assert_eq!(hits, [(EntryId(5), "It was the linker.".to_owned())]);
    let hits = search(&contents(), "the");
    assert_eq!(
        hits.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
        [6, 5, 1]
    );
    assert!(search(&contents(), "  ").is_empty());
}

#[test]
fn a_read_fits_its_bound_as_json_even_when_every_character_is_escaped() {
    // Control characters take six bytes each in JSON.
    let contents = Contents {
        entries: vec![
            agent(1, &"\u{1}".repeat(MAX_BYTES)),
            agent(2, &"\"".repeat(MAX_BYTES)),
        ],
        trimmed: Vec::new(),
    };
    for id in [1, 2] {
        let read = read(&contents, EntryId(id), EntryId(id));
        let encoded = serde_json::to_string(&read.text).unwrap();
        assert!(encoded.len() <= MAX_BYTES + 64, "{}", encoded.len());
        assert!(read.text.contains(" [cut]"));
    }
    assert_eq!(escaped_len("a\"\n\u{1}é"), 1 + 2 + 2 + 6 + 2);
}

#[test]
fn many_trimmed_runs_count_against_the_bound_too() {
    // Every other id trimmed: thousands of runs between kept entries.
    let contents = Contents {
        entries: (0..20_000).map(|at| agent(at * 2 + 2, "hi")).collect(),
        trimmed: (0..20_000)
            .map(|at| Trimmed {
                from: EntryId(at * 2 + 1),
                to: EntryId(at * 2 + 1),
            })
            .collect(),
    };
    let read = read(&contents, EntryId(1), EntryId(40_000));
    assert_eq!(read.entries, MAX_ENTRIES);
    assert!(escaped_len(&read.text) <= MAX_BYTES + 64);
    let runs = Contents {
        entries: Vec::new(),
        trimmed: contents.trimmed.clone(),
    };
    let read = super::read(&runs, EntryId(1), EntryId(40_000));
    assert!(read.next.is_some());
    assert!(escaped_len(&read.text) <= MAX_BYTES + 64);
}
