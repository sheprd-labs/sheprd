use super::*;
use sheprd_chat_protocol::{Choice, ConfigOption, OptionCategory, OptionKind, Usage};

/// When the last setting almost fits, it stays on the first line and
/// truncates its label rather than wrapping onto a line of its own.
#[gpui::test]
fn settings_stay_on_one_line_when_they_fit(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![agent(1, "Looking.")];
    transcript.usage = Some(Usage {
        used: 69_500,
        size: 1_000_000,
        cost: None,
    });
    transcript.options = [
        ("model", OptionCategory::Model, "Opus 5.5"),
        ("effort", OptionCategory::ThoughtLevel, "Auto"),
        ("mode", OptionCategory::Mode, "Bypass Permissions"),
    ]
    .into_iter()
    .map(|(id, category, name)| ConfigOption {
        id: id.into(),
        category,
        name: id.into(),
        description: None,
        kind: OptionKind::Select {
            choices: vec![
                Choice {
                    id: "selected".into(),
                    name: name.into(),
                    description: None,
                    group: None,
                },
                Choice {
                    id: "other".into(),
                    name: "Other".into(),
                    description: None,
                    group: None,
                },
            ],
            current: "selected".into(),
        },
    })
    .collect();
    let _stream = attach_with(
        &view,
        &program,
        ServerMessage::Snapshot {
            cursor: cursor(1, 0),
            transcript: Box::new(transcript),
        },
        cx,
    );
    for mode in [
        ChatLayoutMode::Timeline,
        ChatLayoutMode::Document,
        ChatLayoutMode::Cards,
        ChatLayoutMode::IA,
    ] {
        switch(&view, mode, cx);
        view.update(cx, |view, cx| {
            view.config.chat.max_width = 900;
            cx.notify();
        });
        poll(&view, &program, cx);
        let attach = cx.debug_bounds("chat-attach").unwrap();
        let mode_setting = cx.debug_bounds("chat-setting-mode").unwrap();
        let usage = cx.debug_bounds("chat-usage").unwrap();
        assert!(
            mode_setting.top() < attach.bottom(),
            "{mode:?}: the settings stay on the first line"
        );
        assert!(
            mode_setting.right() <= usage.left(),
            "{mode:?}: the last setting truncates short of usage"
        );
    }
}
