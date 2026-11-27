use sheprd_chat_protocol::{
    Author, Cursor, Entry, EntryId, EntryKind, Image, ImageType, Status, Tool, ToolContent,
    ToolKind, ToolStatus, Transcript,
    sharing::{MAX_RESPONSE_BYTES, Update},
};

use crate::sharing::{
    Content, History, Source,
    tests::{Fixture, agent, clock, snapshot},
};

fn tool(secret: &str) -> EntryKind {
    EntryKind::Tool(Tool {
        call_id: "private-call".into(),
        content: vec![ToolContent::Text {
            text: secret.into(),
        }],
        kind: ToolKind::Execute,
        locations: vec!["private-location".into()],
        parent: Some("private-parent".into()),
        started_ms: Some(1),
        status: ToolStatus::InProgress,
        title: "Test command".into(),
    })
}

#[test]
fn from_now_excludes_old_entries_even_after_their_content_changes_and_on_reconnect() {
    let mut f = Fixture::new();
    f.push(400, tool("old tool secret"));
    f.push(401, agent("old reply"));
    let (invite, guest) = f.share(History::FromNow, Content { tools: true });
    let (cursor, initial) = snapshot(f.read(&guest, None, 0).unwrap());
    assert!(initial.entries.is_empty());
    f.transcript.entries[0].kind = tool("old tool updated with a secret");
    f.cursor.seq += 1;
    assert!(matches!(
        f.read(&guest, Some(cursor), 0).unwrap(),
        Update::Unchanged { .. }
    ));
    f.push(402, agent("new reply"));
    let (_, live) = snapshot(f.read(&guest, None, 0).unwrap());
    assert_eq!(
        live.entries,
        [Entry {
            id: EntryId(1),
            kind: agent("new reply")
        }]
    );
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    let other = f
        .store
        .admit(
            invite.share,
            invite.grant,
            &invite.capability,
            source,
            clock(0),
        )
        .unwrap();
    let (_, resumed) = snapshot(f.read(&other, Some(Cursor::new_epoch()), 0).unwrap());
    assert_eq!(resumed, live);
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert_eq!(
        snapshot(f.store.preview(invite.share, source, clock(0)).unwrap()).1,
        live
    );
}

#[test]
fn only_explicit_text_crosses_the_boundary() {
    let mut f = Fixture::new();
    f.push(
        1,
        EntryKind::User {
            text: "question\n📎 private-file".into(),
            files: 1,
            note: Some("private-context".into()),
        },
    );
    f.push(
        2,
        EntryKind::Thought {
            text: "private-reasoning".into(),
        },
    );
    f.push(
        3,
        EntryKind::Image {
            author: Author::Agent,
            image: Image {
                data: "private-image".into(),
                kind: ImageType::Png,
            },
            caption: None,
        },
    );
    f.push(4, tool("private-tool-output"));
    f.push(5, agent("visible response"));
    f.transcript.auto_accept = true;
    f.transcript.clients = 99;
    f.transcript.status = Status::Failed {
        message: "private-diagnostic".into(),
    };
    let (_, guest) = f.share(History::All, Content::default());
    let (cursor, transcript) = snapshot(f.read(&guest, None, 0).unwrap());
    assert_ne!(cursor.epoch, f.cursor.epoch);
    assert!(transcript.read_only);
    assert_eq!(transcript.entries.len(), 2);
    assert!(!transcript.auto_accept);
    assert_eq!(transcript.clients, 0);
    let bytes = serde_json::to_string(&transcript).unwrap();
    assert!(!bytes.contains("private-"), "{bytes}");
    assert!(!bytes.contains("stable-agent-session"));
    assert!(bytes.contains("question"));
    assert!(bytes.contains("visible response"));
}

#[test]
fn tools_require_opt_in_and_never_carry_local_file_or_terminal_handles() {
    let mut f = Fixture::new();
    let mut kind = tool("visible tool output");
    if let EntryKind::Tool(tool) = &mut kind {
        tool.content.extend([
            ToolContent::Diff {
                new: "private-new".into(),
                old: Some("private-old".into()),
                path: "private-path".into(),
            },
            ToolContent::Terminal {
                id: "private-terminal".into(),
            },
        ]);
    }
    f.push(920, kind);
    let (_, guest) = f.share(History::All, Content { tools: true });
    let (_, transcript) = snapshot(f.read(&guest, None, 0).unwrap());
    let bytes = serde_json::to_string(&transcript).unwrap();
    assert!(bytes.contains("visible tool output"));
    assert!(bytes.contains("shared-1"));
    assert!(!bytes.contains("private-"));
    assert!(!bytes.contains("920"));
}

fn apply(current: &mut Transcript, update: Update) -> Cursor {
    match update {
        Update::Snapshot { cursor, transcript } => {
            *current = *transcript;
            cursor
        }
        Update::Changes { cursor, changes } => {
            for change in changes {
                current.apply(&change).unwrap();
            }
            cursor
        }
        Update::Unchanged { cursor } => cursor,
    }
}

#[test]
fn deltas_reconstruct_exactly_the_filtered_snapshot() {
    let mut f = Fixture::new();
    f.push(10, agent("first"));
    let (_, guest) = f.share(History::All, Content { tools: true });
    let (mut cursor, mut client) = snapshot(f.read(&guest, None, 0).unwrap());
    for step in 0..80 {
        match step % 4 {
            0 => f.push(20 + step, agent("another reply")),
            1 => {
                f.transcript.entries.last_mut().unwrap().kind = tool("updated tool");
                f.cursor.seq += 1;
            }
            2 => {
                f.transcript.status = if step % 8 == 2 {
                    Status::Idle
                } else {
                    Status::Working
                };
                f.cursor.seq += 1;
            }
            _ => {
                f.transcript.entries.remove(0);
                f.transcript.truncated = true;
                f.cursor.seq += 1;
            }
        }
        let update = f.read(&guest, Some(cursor), 0).unwrap();
        if step == 0 {
            assert!(matches!(update, Update::Changes { .. }));
        }
        cursor = apply(&mut client, update);
        let (snapshot_cursor, expected) = snapshot(f.read(&guest, None, 0).unwrap());
        assert_eq!(snapshot_cursor, cursor);
        assert_eq!(client, expected, "step {step}");
    }
}

#[test]
fn a_hidden_entry_becoming_visible_resets_the_public_id_space() {
    let mut f = Fixture::new();
    f.push(
        10,
        EntryKind::Thought {
            text: "hidden".into(),
        },
    );
    f.push(20, agent("visible"));
    let (_, guest) = f.share(History::All, Content::default());
    let (cursor, _) = snapshot(f.read(&guest, None, 0).unwrap());
    f.transcript.entries[0].kind = agent("now visible");
    f.cursor.seq += 1;
    let (next, transcript) = snapshot(f.read(&guest, Some(cursor), 0).unwrap());
    assert_ne!(cursor.epoch, next.epoch);
    assert_eq!(
        transcript.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        [EntryId(1), EntryId(2)]
    );
}

#[test]
fn encoded_response_and_unicode_text_stay_bounded() {
    let mut f = Fixture::new();
    for id in 0..600 {
        f.push(id, agent(&"\0🍐".repeat(20_000)));
    }
    let (_, guest) = f.share(History::All, Content::default());
    let update = f.read(&guest, None, 0).unwrap();
    assert!(serde_json::to_vec(&update).unwrap().len() <= MAX_RESPONSE_BYTES);
    let (_, transcript) = snapshot(update);
    assert!(transcript.truncated);
    assert!(!transcript.entries.is_empty());
    assert!(transcript.entries.len() < f.transcript.entries.len());
}

#[test]
fn malformed_attachment_metadata_is_not_projected() {
    let mut f = Fixture::new();
    f.push(
        1,
        EntryKind::User {
            text: "private-file".into(),
            files: 1,
            note: None,
        },
    );
    let (_, guest) = f.share(History::All, Content::default());
    assert!(
        snapshot(f.read(&guest, None, 0).unwrap())
            .1
            .entries
            .is_empty()
    );
}
