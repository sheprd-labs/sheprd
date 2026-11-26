//! A connected chat with no entries still tells the user what to do.

use super::*;
use gpui::TestAppContext;
use sheprd_client::ConnectTarget;

#[gpui::test]
fn the_first_message_replaces_the_empty_chat_prompt(cx: &mut TestAppContext) {
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = overlay::attach_with(&view, &program, snapshot(cursor(1, 0), Vec::new()), cx);

    assert!(cx.debug_bounds("chat-empty").is_some());
    assert!(cx.debug_bounds("chat-composer").is_some());

    send(
        &mut stream,
        &change(
            Change::Push {
                entry: agent(1, "Hello."),
            },
            cursor(1, 1),
        ),
    );
    wait_until(|| {
        overlay::poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane("w1:p1")
                .is_some_and(|chat| !chat.session.transcript().entries.is_empty())
        })
    });
    assert!(cx.debug_bounds("chat-empty").is_none());
}
