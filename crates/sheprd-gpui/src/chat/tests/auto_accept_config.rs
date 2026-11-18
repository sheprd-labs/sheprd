//! The opt-in policy applies at attachment, not just on a GUI start path.
use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::SheprdWindow;
use gpui::{Entity, TestAppContext, VisualTestContext};
use sheprd_chat_protocol::Command;
use sheprd_client::ConnectTarget;

fn configure(view: &Entity<SheprdWindow>, enabled: bool, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        view.config.chat = toml::from_str(&format!("auto_accept = {enabled}\n")).unwrap();
    });
}

fn next(stream: &mut Peer) -> (sheprd_chat_protocol::CommandId, Command) {
    match read(stream) {
        Some(ClientMessage::Command { id, command }) => (id, command),
        other => panic!("expected a command, got {other:?}"),
    }
}

// The marker is queued behind every earlier command, avoiding timing-based
// assertions that an asynchronous writer has sent nothing yet.
fn marker(view: &Entity<SheprdWindow>, stream: &mut Peer, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.chat_send("w1:p1", Command::Cancel, cx));
    assert_eq!(next(stream).1, Command::Cancel);
}

#[gpui::test]
fn enabled_sends_once_after_attach_and_respects_a_manual_off(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    configure(&view, true, cx);
    poll(&view, &program, cx);
    let (mut stream, _) = program.accept();
    send(&mut stream, &welcome());
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats.pane("w1:p1").unwrap().session.agent() == "Claude"
        })
    });
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert!(!pane.attached());
        assert!(
            pane.session.resend().is_empty(),
            "nothing before the transcript"
        );
    });
    send(&mut stream, &snapshot(cursor(1, 0), vec![]));
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| view.chats.pane("w1:p1").unwrap().attached())
    });
    // Queue a marker so a missing auto-accept fails immediately, not on timeout.
    view.update(cx, |view, cx| view.chat_send("w1:p1", Command::Cancel, cx));
    let (id, command) = next(&mut stream);
    assert_eq!(command, Command::SetAutoAccept { on: true });
    assert_eq!(next(&mut stream).1, Command::Cancel);
    send(&mut stream, &ServerMessage::Ack { id, rejected: None });
    send(
        &mut stream,
        &change(Change::AutoAccept { on: false }, cursor(1, 1)),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats.pane("w1:p1").unwrap().session.since() == Some(cursor(1, 1))
        })
    });
    for _ in 0..3 {
        poll(&view, &program, cx);
    }
    marker(&view, &mut stream, cx);
    // Switching away and back rebuilds the pane, but not the session policy.
    view.update(cx, |view, _| {
        view.chats
            .reconcile(vec![], Instant::now(), &mut |_, _, _| unreachable!());
    });
    let mut reattached = attach_with(&view, &program, snapshot(cursor(1, 2), vec![]), cx);
    marker(&view, &mut reattached, cx);
    // A new stream epoch is a new session and gets its own one-time policy.
    send(&mut reattached, &snapshot(cursor(2, 0), vec![]));
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats.pane("w1:p1").unwrap().session.since() == Some(cursor(2, 0))
        })
    });
    view.update(cx, |view, cx| view.chat_send("w1:p1", Command::Cancel, cx));
    assert_eq!(next(&mut reattached).1, Command::SetAutoAccept { on: true });
    assert_eq!(next(&mut reattached).1, Command::Cancel);
}

#[gpui::test]
fn disabled_sends_nothing(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    configure(&view, false, cx);
    let mut stream = attach_with(&view, &program, snapshot(cursor(1, 0), vec![]), cx);
    for _ in 0..3 {
        poll(&view, &program, cx);
    }
    marker(&view, &mut stream, cx);
}

#[gpui::test]
fn enabling_also_updates_an_already_attached_chat(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, snapshot(cursor(1, 0), vec![]), cx);
    configure(&view, true, cx);
    poll(&view, &program, cx);
    view.update(cx, |view, cx| view.chat_send("w1:p1", Command::Cancel, cx));
    assert_eq!(next(&mut stream).1, Command::SetAutoAccept { on: true });
    assert_eq!(next(&mut stream).1, Command::Cancel);
    poll(&view, &program, cx);
    marker(&view, &mut stream, cx);
}

#[gpui::test]
fn already_auto_accepting_is_not_sent_or_overridden_later(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    configure(&view, true, cx);
    let mut message = snapshot(cursor(1, 0), vec![]);
    if let ServerMessage::Snapshot { transcript, .. } = &mut message {
        transcript.auto_accept = true;
    }
    let mut stream = attach_with(&view, &program, message, cx);
    marker(&view, &mut stream, cx);
    send(
        &mut stream,
        &change(Change::AutoAccept { on: false }, cursor(1, 1)),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats.pane("w1:p1").unwrap().session.since() == Some(cursor(1, 1))
        })
    });
    marker(&view, &mut stream, cx);
}

#[gpui::test]
fn read_only_mirrors_are_left_alone(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    configure(&view, true, cx);
    let mut message = snapshot(cursor(1, 0), vec![]);
    if let ServerMessage::Snapshot { transcript, .. } = &mut message {
        transcript.read_only = true;
    }
    let _stream = attach_with(&view, &program, message, cx);
    poll(&view, &program, cx);
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert!(pane.error.is_none());
        assert!(pane.session.resend().is_empty());
    });
}
