use super::*;
use crate::sidebar::layout_tests::fixture_window;
use core::prelude::v1::test;
use sheprd_chat_protocol::{Entry, EntryId};
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn document_prompt_body_keeps_normal_text_below_its_speaker_label(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let entry = Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            guest: None,
            text: "A normal prompt".into(),
            files: 0,
            note: None,
        },
    };
    let measured = Rc::new(RefCell::new(None));
    let body_style = measured.clone();
    let expected = view.read_with(cx, |view, _| view.chat_text_size());
    cx.draw(Point::default(), size(px(800.), px(600.)), |_, cx| {
        let content = div().child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| *body_style.borrow_mut() = Some(window.text_style()),
            )
            .size(px(1.)),
        );
        view.read_with(cx, |view, _| {
            Document.row(
                Row {
                    entry: &entry,
                    previous: None,
                    previous_guest: None,
                    next: Some(Side::Agent),
                    agent: "Codex",
                    live: false,
                    current: true,
                    expanded: false,
                    tool: None,
                    content,
                },
                &view.chat_look(),
            )
        })
    });
    let style = measured.borrow();
    let Some(style) = style.as_ref() else {
        panic!("prompt body was not drawn")
    };
    assert_eq!(style.font_weight, FontWeight::NORMAL);
    assert_eq!(style.font_size, px(expected).into());
}

#[test]
fn a_guest_speaks_anew_under_its_own_name() {
    let bob = sheprd_chat_protocol::Guest::new("Bob", "tma");
    let entry = Entry {
        id: EntryId(2),
        kind: EntryKind::User {
            guest: Some(bob.clone()),
            text: "hi".into(),
            files: 0,
            note: None,
        },
    };
    let row = |previous_guest| Row {
        entry: &entry,
        previous: Some(Side::User),
        previous_guest,
        next: None,
        agent: "Codex",
        live: false,
        current: true,
        expanded: false,
        tool: None,
        content: div(),
    };
    // After the person's own words, the guest starts a new speaker; after
    // the same guest, it goes on.
    assert!(row(None).speaker());
    assert!(!row(Some(&bob)).speaker());
    // Fluent isolates each name it places.
    let name = row(None).user_name().replace(['\u{2068}', '\u{2069}'], "");
    assert_eq!(name, "Bob via tma · guest");
}
