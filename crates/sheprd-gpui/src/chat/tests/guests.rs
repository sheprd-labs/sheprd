//! A guest's prompt: named in the transcript, and shown in full while it
//! waits for the person, who sends it on or declines it.

use super::*;
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{
    Command, GUEST_ALLOW, GUEST_DENY, Guest, GuestPrompt, Permission, PermissionId, PermissionKind,
    PermissionOption,
};
use sheprd_client::ConnectTarget;

fn bob() -> Guest {
    Guest::new("Bob", "tma")
}

fn waiting() -> Permission {
    Permission {
        call_id: None,
        id: PermissionId(7),
        options: vec![
            PermissionOption {
                id: GUEST_ALLOW.into(),
                kind: PermissionKind::AllowOnce,
                name: "Send to the agent".into(),
            },
            PermissionOption {
                id: GUEST_DENY.into(),
                kind: PermissionKind::RejectOnce,
                name: "Decline".into(),
            },
        ],
        question: None,
        guest: Some(Box::new(GuestPrompt {
            guest: bob(),
            text: "first line\n\nand a third".into(),
        })),
        title: "Bob via tma asks to send: first line".into(),
    }
}

#[gpui::test]
fn a_guests_prompt_is_named_and_waits_in_full_for_the_person(cx: &mut TestAppContext) {
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    let program = Program::new();
    let ServerMessage::Snapshot {
        cursor,
        mut transcript,
    } = snapshot(
        cursor(1, 0),
        vec![Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                guest: Some(bob()),
                text: "can you add a test?".into(),
                files: 0,
                note: None,
            },
        }],
    )
    else {
        unreachable!();
    };
    transcript.permissions.push(waiting());
    let mut stream = overlay::attach_with(
        &view,
        &program,
        ServerMessage::Snapshot { cursor, transcript },
        cx,
    );
    wait_until(|| {
        cx.run_until_parked();
        overlay::poll(&view, &program, cx);
        cx.debug_bounds("chat-guest-ask-7").is_some()
    });
    assert!(cx.debug_bounds("chat-guest-1").is_some());

    let allow = cx.debug_bounds("chat-answer-0").unwrap();
    cx.simulate_click(allow.center(), Modifiers::default());
    let Some(ClientMessage::Command { command, .. }) = read(&mut stream) else {
        panic!("a command");
    };
    assert_eq!(
        command,
        Command::Answer {
            option: GUEST_ALLOW.into(),
            permission: PermissionId(7),
        }
    );
}

#[test]
fn a_guest_prompts_tabs_show_as_spaces_on_its_card() {
    assert_eq!(view::guest::guest_line("do\tnot"), "do    not");
}
