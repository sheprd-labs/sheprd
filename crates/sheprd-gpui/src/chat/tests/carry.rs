//! A chat's agent starting another chat gets one like its own: the same
//! agent and settings, carried in the new tab's environment.

use super::{
    overlay::{attach_with, open},
    start::local_window,
    *,
};
use crate::window::MockPeer;
use gpui::TestAppContext;
use serde_json::json;
use sheprd_chat_protocol::{
    Agent, Choice, ConfigOption, Modes, OptionCategory, OptionKind, OptionValue, SETTINGS_ENV,
    Setting, Settings,
};
use sheprd_client::ConnectTarget;

fn settings() -> Settings {
    Settings {
        agent: "claude".into(),
        auto_accept: false,
        mode: Some("plan".into()),
        options: vec![Setting {
            id: "model".into(),
            value: OptionValue::Choice { id: "opus".into() },
        }],
    }
}

#[gpui::test]
fn a_carried_chat_starts_with_the_settings_in_its_environment(cx: &mut TestAppContext) {
    let mut peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            view.start_chat(Agent::Claude, None, Some(&settings()))
                .unwrap();
        })
    });
    let tab = peer.request();
    let env = &tab["params"]["env"];
    assert_eq!(env["HERDR_ACP"], "/opt/herdr-acp");
    let carried = Settings::decode(env[SETTINGS_ENV].as_str().unwrap()).unwrap();
    assert_eq!(carried, settings());
}

#[gpui::test]
fn a_chat_without_settings_to_carry_has_none_in_its_environment(cx: &mut TestAppContext) {
    let mut peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            view.start_chat(Agent::Claude, None, None).unwrap();
        })
    });
    let tab = peer.request();
    assert_eq!(tab["params"]["env"], json!({"HERDR_ACP": "/opt/herdr-acp"}));
}

#[gpui::test]
fn the_chat_in_a_pane_is_read_as_its_agent_and_settings(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.modes = Some(Modes {
        available: Vec::new(),
        current: "plan".into(),
    });
    transcript.options = vec![ConfigOption {
        category: OptionCategory::Model,
        description: None,
        id: "model".into(),
        kind: OptionKind::Select {
            choices: vec![Choice {
                description: None,
                group: None,
                id: "opus".into(),
                name: "Opus".into(),
            }],
            current: "opus".into(),
        },
        name: "Model".into(),
    }];
    let _stream = attach_with(
        &view,
        &program,
        ServerMessage::Snapshot {
            cursor: cursor(1, 0),
            transcript: Box::new(transcript),
        },
        cx,
    );
    view.read_with(cx, |view, _| {
        let pane = view.live.snapshot.as_ref().unwrap().agents[0]
            .pane_id
            .clone();
        assert_eq!(view.like_chat(&pane), Some((Agent::Claude, settings())));
        assert_eq!(view.like_chat("w9:p9"), None);
    });
}
