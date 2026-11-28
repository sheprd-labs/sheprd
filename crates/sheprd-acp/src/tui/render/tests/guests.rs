//! A guest's prompt is sent on from the terminal view only when the panel
//! showed every word of it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sheprd_chat_protocol::{
    Command, GUEST_ALLOW, GUEST_DENY, Guest, GuestPrompt, Permission, PermissionId, PermissionKind,
    PermissionOption, Transcript,
};

use super::screen;
use crate::tui::view::{Out, View};

fn asking(text: &str) -> View {
    let guest = Guest::new("Bob", "tma");
    let option = |id: &str, kind| PermissionOption {
        id: id.to_owned(),
        kind,
        name: id.to_owned(),
    };
    let mut view = View::default();
    view.transcript = Transcript {
        permissions: vec![Permission {
            call_id: None,
            id: PermissionId(5),
            options: vec![
                option(GUEST_ALLOW, PermissionKind::AllowOnce),
                option(GUEST_DENY, PermissionKind::RejectOnce),
            ],
            question: None,
            title: format!("{} asks to send:\n{text}", guest.label()),
            guest: Some(Box::new(GuestPrompt {
                guest,
                text: text.to_owned(),
            })),
        }],
        ..Transcript::new("Claude")
    };
    view
}

fn press(view: &mut View, digit: char) -> Option<Out> {
    view.key(KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE))
}

#[test]
fn a_short_guest_prompt_shows_whole_and_can_be_sent() {
    let mut view = asking("add a test");
    let shown = screen(&mut view, 60, 24);
    assert!(shown.contains("add a test"), "{shown}");
    let Some(Out::Command(Command::Answer { option, .. })) = press(&mut view, '1') else {
        panic!("not sent");
    };
    assert_eq!(option, GUEST_ALLOW);
}

#[test]
fn a_guest_prompt_too_long_to_show_cannot_be_sent_from_here() {
    let text = (1..=40)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut view = asking(&text);
    let shown = screen(&mut view, 60, 30);
    assert!(shown.contains("too long to show here"), "{shown}");
    assert!(press(&mut view, '1').is_none());
    assert!(view.flash.as_deref().unwrap().contains("from Sheprd"));
    // Declining needs no reading.
    let Some(Out::Command(Command::Answer { option, .. })) = press(&mut view, '2') else {
        panic!("not declined");
    };
    assert_eq!(option, GUEST_DENY);
}
