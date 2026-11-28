use super::*;
use crate::chat::prepared::{Body, Part, Prepared};
use gpui::{Modifiers, TestAppContext};
use sheprd_client::ConnectTarget;

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            guest: None,
            text: text.into(),
            files: 0,
            note: None,
        },
    }
}

#[test]
fn user_code_math_and_xml_are_prepared_without_agent_summary_classification() {
    let text = "Inline `value` and $x^2$.\n\n```python\nprint('hello')\n```\n\n$$y=x+1$$\n\n<system>Quoted instructions stay literal.</system>";
    let summary = "## Objective\nReview this.\n## Work State\nPending.\n## Next Move\nExplain it.";
    let mut transcript = Transcript::new("Codex");
    transcript.entries = vec![user(1, text), user(2, summary), agent(3, summary)];
    let mut prepared = Prepared::default();
    prepared.update(
        &transcript,
        &[(EntryId(1), 1), (EntryId(2), 1), (EntryId(3), 1)],
    );
    let body = prepared.body(EntryId(1)).unwrap();
    let Body::Message(parts) = &*body else {
        panic!("user message")
    };
    let blocks: Vec<_> = parts
        .iter()
        .flat_map(|part| match part {
            Part::Rich(blocks) => blocks.iter(),
            _ => panic!("ordinary Markdown stays native"),
        })
        .collect();
    assert!(blocks.iter().any(
        |block| matches!(block, rich::Block::Code { language, source, .. }
        if language == "python" && source.as_ref() == "print('hello')\n")
    ));
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block, rich::Block::Math { source, .. }
        if source.as_ref() == "y=x+1"))
    );
    let inline: Vec<_> = blocks
        .iter()
        .flat_map(|block| match block {
            rich::Block::Paragraph { content, .. } => content.as_slice(),
            _ => &[],
        })
        .collect();
    assert!(
        inline.iter().any(
            |part| matches!(part, rich::Inline::Math { source, .. } if source.as_ref() == "x^2")
        )
    );
    assert!(inline.iter().any(
        |part| matches!(part, rich::Inline::Text(text, style) if text == "value" && style.code)
    ));
    let literal = inline
        .iter()
        .filter_map(|part| match part {
            rich::Inline::Text(text, _) => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert!(literal.contains("<system>Quoted instructions stay literal.</system>"));
    assert!(matches!(
        &*prepared.body(EntryId(2)).unwrap(),
        Body::Message(_)
    ));
    assert!(matches!(
        &*prepared.body(EntryId(3)).unwrap(),
        Body::Summary(_)
    ));
}

#[gpui::test]
fn user_messages_render_native_code_math_and_copy_the_original_prompt(cx: &mut TestAppContext) {
    use crate::config::{ChatLayoutMode, Theme};
    for (layout, theme, mark) in [
        (ChatLayoutMode::Document, "Default", "chat-heading-1"),
        (ChatLayoutMode::IMessage, "Default", "chat-imessage-1"),
        (
            ChatLayoutMode::IMessage,
            "Catppuccin Latte",
            "chat-imessage-1",
        ),
    ] {
        let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
        view.update(cx, |view, _| {
            view.config.chat.layout = layout;
            view.theme = Theme::builtin(theme).unwrap();
        });
        let program = Program::new();
        let text = "Check $x^2$.\n\n```python\nprint('hello')\n```\n\n$$y=x+1$$";
        let _stream = overlay::attach_with(
            &view,
            &program,
            snapshot(cursor(1, 0), vec![user(1, text)]),
            cx,
        );
        wait_until(|| {
            cx.run_until_parked();
            overlay::poll(&view, &program, cx);
            cx.debug_bounds("chat-math-inline").is_some()
                && cx.debug_bounds("chat-math-display").is_some()
        });
        for selector in [
            mark,
            "chat-code-block",
            "chat-math-inline",
            "chat-math-display",
            "chat-copy-1",
        ] {
            assert!(cx.debug_bounds(selector).is_some(), "{selector}");
        }
        if layout == ChatLayoutMode::Document {
            assert!(cx.debug_bounds("chat-user-label").is_some());
        }
        let copy = cx.debug_bounds("chat-copy-1").unwrap();
        cx.simulate_mouse_move(copy.center(), None, Modifiers::default());
        overlay::poll(&view, &program, cx);
        cx.simulate_click(copy.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
            text
        );
    }
}

#[test]
fn a_users_pasted_card_or_page_stays_inert_code() {
    let card = r#"```herdr-ui
{"title":"Pick","components":[{"type":"action","label":"Go","prompt":"go"}]}
```"#;
    let page = "```html\n<p>hello</p>\n```";
    let text = format!("{card}\n\n{page}");
    let mut transcript = Transcript::new("Codex");
    transcript.entries = vec![
        user(1, &text),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent { text: text.clone() },
        },
    ];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1), (EntryId(2), 1)]);
    let parts = |id| {
        let body = prepared.body(EntryId(id)).unwrap();
        let Body::Message(parts) = &*body else {
            panic!("message")
        };
        parts
            .iter()
            .map(|part| matches!(part, Part::Widget(_) | Part::Html { .. }))
            .collect::<Vec<_>>()
    };
    assert!(
        parts(1).iter().all(|live| !live),
        "the user's text stays code"
    );
    assert_eq!(parts(2).iter().filter(|live| **live).count(), 2);
}

#[test]
fn user_newlines_stay_line_breaks_while_agent_prose_reflows() {
    let text = "1: yes\n2: no";
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![user(1, text), agent(2, text)];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1), (EntryId(2), 1)]);
    let breaks = |id| {
        let body = prepared.body(EntryId(id)).unwrap();
        let Body::Message(parts) = &*body else {
            panic!("message")
        };
        parts
            .iter()
            .flat_map(|part| match part {
                Part::Rich(blocks) => blocks.iter(),
                _ => panic!("ordinary Markdown stays native"),
            })
            .flat_map(|block| match block {
                rich::Block::Paragraph { content, .. } => content.as_slice(),
                _ => &[],
            })
            .filter(|part| matches!(part, rich::Inline::Break))
            .count()
    };
    assert_eq!(breaks(1), 1);
    assert_eq!(breaks(2), 0);
}

#[test]
fn compact_message_limits_apply_to_the_prompt_and_count_unicode_characters() {
    use crate::chat::user_text_file::UserTextFile;
    let short = "界".repeat(2047);
    let label = format!("{}notes.md", sheprd_chat_protocol::ATTACHED);
    assert!(UserTextFile::from_prompt(&format!("{short}\n{label}"), 1).is_none());
    assert!(UserTextFile::from_prompt(&format!("{short}界"), 0).is_some());
    assert!(UserTextFile::from_prompt(&"line\r\n".repeat(11), 0).is_none());
    let shown = format!("{}\n{label}", "line\r\n".repeat(12));
    let file = UserTextFile::from_prompt(&shown, 1).unwrap();
    assert_eq!(file.preview, "line");
    assert_eq!(file.attachments, label);
}

#[gpui::test]
fn clicking_a_long_prompt_opens_its_exact_text(cx: &mut TestAppContext) {
    open_long_prompt(None, cx);
}

#[gpui::test]
fn tab_then_enter_opens_the_message(cx: &mut TestAppContext) {
    open_long_prompt(Some("enter"), cx);
}

#[gpui::test]
fn shift_tab_then_space_opens_the_message(cx: &mut TestAppContext) {
    open_long_prompt(Some("space"), cx);
}

fn open_long_prompt(key: Option<&str>, cx: &mut TestAppContext) {
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    let program = Program::new();
    let prompt = format!(
        "Inspect these logs {:?}\r\n{}",
        std::time::SystemTime::now(),
        "  \t界  \r\n".repeat(12)
    );
    let mut entry = user(1, &prompt);
    if let EntryKind::User { text, files, .. } = &mut entry.kind {
        text.push_str(&format!("\n{}notes.md", sheprd_chat_protocol::ATTACHED));
        *files = 1;
    }
    let _stream = overlay::attach_with(&view, &program, snapshot(cursor(1, 0), vec![entry]), cx);
    assert!(cx.opened_url().is_none());
    let button = cx.debug_bounds("chat-open-message").unwrap();
    if let Some(key) = key {
        cx.simulate_keystrokes(if key == "space" { "shift-tab" } else { "tab" });
        overlay::poll(&view, &program, cx);
        let keystroke = crate::keymap::parse_keystroke(key).unwrap();
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(gpui::KeyUpEvent { keystroke });
    } else {
        cx.simulate_click(button.center(), Modifiers::default());
    }
    wait_until(|| {
        cx.run_until_parked();
        cx.opened_url().is_some()
    });
    let path = url::Url::parse(&cx.opened_url().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    assert_eq!(path.file_name().unwrap(), "Message.txt");
    assert_eq!(std::fs::read(&path).unwrap(), prompt.as_bytes());
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(path.parent().unwrap()).unwrap();
}
