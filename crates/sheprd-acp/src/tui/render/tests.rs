#![allow(clippy::unwrap_used, clippy::expect_used)]

use ratatui::{Terminal, backend::TestBackend};
use sheprd_chat_protocol::{
    Cursor, Entry, EntryId, EntryKind, Permission, PermissionId, PermissionOption, Queued,
    ServerMessage, Transcript,
};

use super::*;

fn screen(view: &mut View, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, view, 0)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn busy() -> View {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.clients = 1;
    transcript.entries = vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "please fix the failing test in the parser module".to_owned(),
                files: 0,
                note: None,
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "Looking at it now.".to_owned(),
            },
        },
    ];
    transcript.permissions = vec![Permission {
        call_id: None,
        id: PermissionId(1),
        options: vec![PermissionOption {
            id: "allow".to_owned(),
            kind: PermissionKind::AllowOnce,
            name: "Allow once".to_owned(),
        }],
        question: None,
        title: "Run cargo test -p parser".to_owned(),
    }];
    transcript.queued = vec![Queued {
        attachments: Vec::new(),
        id: sheprd_chat_protocol::CommandId::random(),
        held: false,
        steered: false,
        text: "and then commit".to_owned(),
    }];
    let mut view = View::default();
    view.receive(ServerMessage::Snapshot {
        cursor: Cursor {
            epoch: uuid::Uuid::nil(),
            seq: 0,
        },
        transcript: Box::new(transcript),
    });
    view
}

#[test]
fn a_busy_session_shows_everything_it_needs() {
    let screen = screen(&mut busy(), 80, 20);
    for expected in [
        "Claude",
        "waiting for you",
        "1 client attached",
        "◆ you",
        " │ please fix",
        "◇ Claude",
        "Looking at it now.",
        "↳ queued: and then commit",
        "Permission",
        "1. Allow once",
    ] {
        assert!(
            screen.contains(expected),
            "{expected} missing from\n{screen}"
        );
    }
}

#[test]
fn narrow_and_short_terminals_still_draw() {
    for (width, height) in [(20, 6), (12, 4), (1, 1), (200, 3)] {
        screen(&mut busy(), width, height);
    }
    let narrow = screen(&mut busy(), 30, 24);
    assert!(narrow.contains(" Claude  waiting for you"), "{narrow}");
    let medium = screen(&mut busy(), 60, 24);
    assert!(medium.contains("1 client attached"), "{medium}");
    assert!(medium.contains("Ctrl+D quit"), "{medium}");
    let wide = screen(&mut busy(), 100, 24);
    assert!(wide.contains("Enter send · Esc cancel"), "{wide}");
}

fn empty(clients: u32) -> View {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.clients = clients;
    transcript.session = Some(sheprd_chat_protocol::SessionInfo {
        continued: None,
        cwd: Some("/srv/projects/parser".to_owned()),
        id: "s".to_owned(),
        title: None,
    });
    let mut view = View::default();
    view.receive(ServerMessage::Snapshot {
        cursor: Cursor {
            epoch: uuid::Uuid::nil(),
            seq: 0,
        },
        transcript: Box::new(transcript),
    });
    view
}

#[test]
fn an_empty_chat_welcomes_with_where_it_runs() {
    let screen = screen(&mut empty(0), 100, 24);
    for expected in [
        "◇ Claude  ready",
        "/srv/projects/parser",
        "Enter send",
        "Sheprd's chat is not attached",
        "❯ Ask Claude…",
    ] {
        assert!(
            screen.contains(expected),
            "{expected} missing from\n{screen}"
        );
    }
    let attached = self::screen(&mut empty(1), 100, 24);
    assert!(attached.contains("● 1 client attached"), "{attached}");
    assert!(!attached.contains("not attached"), "{attached}");
}

#[test]
fn a_flash_shows_in_the_status_bar() {
    let mut view = busy();
    view.flash = Some("Press Ctrl+C again to quit".to_owned());
    let wide = screen(&mut view, 100, 20);
    assert!(wide.contains("Press Ctrl+C again to quit"), "{wide}");
    let narrow = screen(&mut view, 30, 20);
    assert!(narrow.contains("Press Ctrl+C again to quit"), "{narrow}");
}

#[test]
fn the_view_scrolls_back_and_clamps() {
    let mut view = busy();
    view.scroll = 1000;
    let screen = screen(&mut view, 80, 20);
    assert!(screen.contains(" │ please fix"));
    assert!(view.scroll < 1000);
}

#[test]
fn the_composer_marks_where_the_prompt_goes() {
    let mut view = busy();
    view.transcript.permissions.clear();
    let screen = screen(&mut view, 60, 16);
    assert!(screen.contains("❯ Type to queue a prompt…"), "{screen}");
}
