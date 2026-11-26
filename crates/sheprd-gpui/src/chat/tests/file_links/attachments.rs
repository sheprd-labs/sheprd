use crate::chat::tests::file_links::document_link;
use crate::chat::tests::visual::message;
use crate::chat::tests::{Program, agent, cursor, overlay, snapshot, wait_until};
use gpui::{Modifiers, TestAppContext};
use sheprd_client::ConnectTarget;

#[test]
fn standalone_files_become_cards_without_reinterpreting_prose_or_code() {
    for name in [
        "clip.mp4",
        "sheet.xlsx",
        "report.pdf",
        "archive.zip",
        "data.unknown",
    ] {
        let (_, shape) = message(&format!("Here it is:\n\n[{name}](/tmp/{name})\n\nDone."));
        assert_eq!(
            shape,
            [
                "text:Here it is:".to_owned(),
                format!("file:/tmp/{name}"),
                "text:Done.".to_owned()
            ]
        );
    }
    for text in [
        "See [clip](/tmp/clip.mp4) here.",
        "[source](/tmp/main.rs:12)",
        "[website](https://example.com)",
        "[unsafe](javascript:alert)",
        "[network](file://other-host/share/clip.mp4)",
        "`[clip](/tmp/clip.mp4)`",
        "```md\n[clip](/tmp/clip.mp4)\n```",
        "    [clip](/tmp/clip.mp4)",
        "[unfinished](/tmp/clip.mp4",
        "[one](/tmp/one) [two](/tmp/two)",
    ] {
        let (_, shape) = message(text);
        assert!(
            !shape.iter().any(|part| part.starts_with("file:")),
            "{text}"
        );
    }
    let (_, shape) = message("[Home — **clip**](</tmp/Home (Window).xlsx>)");
    assert_eq!(shape, ["file:/tmp/Home (Window).xlsx"]);
}

#[test]
fn file_cards_are_bounded_and_excess_links_stay_visible() {
    let limit = sheprd_chat_protocol::MAX_ATTACHMENTS;
    let source = (0..limit + 1)
        .map(|i| format!("[file-{i}](/tmp/file-{i})\n\n"))
        .collect::<String>();
    let (_, shape) = message(&source);
    assert_eq!(
        shape
            .iter()
            .filter(|part| part.starts_with("file:"))
            .count(),
        limit
    );
    assert_eq!(shape.last().unwrap(), &format!("text:file-{limit}"));
}

#[gpui::test]
fn file_card_label_opens_in_the_system_viewer_even_with_editor_preference(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Home (Window).xlsx");
    std::fs::write(&path, b"synthetic attachment").unwrap();
    let source = document_link("Home spreadsheet", &path);
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| {
        view.config.open_files_in = crate::config::FileTarget::Editor;
    });
    let program = Program::new();
    let _stream = overlay::attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &source)]),
        cx,
    );
    cx.run_until_parked();
    assert!(cx.debug_bounds("chat-file-card").is_some());
    assert!(cx.opened_url().is_none(), "rendering never opens the file");
    let button = cx.debug_bounds("chat-prose").unwrap();
    cx.simulate_click(
        gpui::point(button.left() + gpui::px(10.), button.center().y),
        Modifiers::default(),
    );
    wait_until(|| {
        cx.run_until_parked();
        cx.opened_url().is_some()
    });
    let opened = url::Url::parse(&cx.opened_url().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(opened).unwrap(),
        std::fs::canonicalize(path).unwrap()
    );
}

#[test]
fn cards_preserve_reference_links_on_both_sides_and_can_use_references() {
    use crate::chat::{
        prepared::{Body, Part},
        rich::{Block, Inline},
    };
    use sheprd_chat_protocol::EntryId;
    for source in [
        "[docs][ref]\n\n[file](/tmp/report.pdf)\n\n[ref]: https://example.com",
        "[ref]: https://example.com\n\n[file](/tmp/report.pdf)\n\n[docs][ref]",
        "[docs][ref]\n\n[file][artifact]\n\n[docs][ref]\n\n[ref]: https://example.com\n[artifact]: /tmp/report.pdf",
    ] {
        let (prepared, shape) = message(source);
        assert!(
            shape.iter().any(|part| part == "file:/tmp/report.pdf"),
            "{source}"
        );
        let body = prepared.body(EntryId(1)).unwrap();
        let Body::Message(parts) = &*body else {
            panic!("message")
        };
        let links: Vec<_> = parts
            .iter()
            .filter_map(|part| match part {
                Part::Rich(blocks) => Some(blocks),
                _ => None,
            })
            .flat_map(|blocks| blocks.iter())
            .filter_map(|block| match block {
                Block::Paragraph { content, .. } => Some(content),
                _ => None,
            })
            .flatten()
            .filter_map(|inline| match inline {
                Inline::Text(text, style) if text == "docs" => style.link.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(links.len(), source.matches("[docs]").count(), "{source}");
        assert!(links.iter().all(|link| *link == "https://example.com/"));
    }
}

#[gpui::test]
fn file_card_open_action_uses_the_system_viewer_even_with_editor_preference(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Home (Window).xlsx");
    std::fs::write(&path, b"synthetic attachment").unwrap();
    let source = document_link("Home spreadsheet", &path);
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| {
        view.config.open_files_in = crate::config::FileTarget::Editor;
    });
    let program = Program::new();
    let _stream = overlay::attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &source)]),
        cx,
    );
    cx.run_until_parked();
    assert!(cx.debug_bounds("chat-file-card").is_some());
    assert!(cx.opened_url().is_none(), "rendering never opens the file");
    let button = cx.debug_bounds("chat-file-open").unwrap();
    cx.simulate_click(
        gpui::point(button.left() + gpui::px(10.), button.center().y),
        Modifiers::default(),
    );
    wait_until(|| {
        cx.run_until_parked();
        cx.opened_url().is_some()
    });
    let opened = url::Url::parse(&cx.opened_url().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(opened).unwrap(),
        std::fs::canonicalize(path).unwrap()
    );
}

/// GPUI's test platform leaves `reveal_path` unimplemented, so its panic is
/// the proof that the click reached the file manager.
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn file_card_reveal_action_sits_beside_open_and_reveals_the_file(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("clip.mp4");
    std::fs::write(&path, b"synthetic attachment").unwrap();
    let source = document_link("Clip", &path);
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = overlay::attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &source)]),
        cx,
    );
    cx.run_until_parked();
    let open = cx.debug_bounds("chat-file-open").unwrap();
    let reveal = cx.debug_bounds("chat-file-reveal").unwrap();
    assert!(reveal.left() >= open.right(), "{open:?} {reveal:?}");
    cx.simulate_click(reveal.center(), Modifiers::default());
    wait_until(|| {
        cx.run_until_parked();
        assert!(cx.opened_url().is_none(), "revealing never opens the file");
        false
    });
}
