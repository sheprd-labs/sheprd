//! Settings carried from the chat that asked for this one.

use sheprd_chat_protocol::{OptionValue, Setting, Settings};

use super::*;

fn options(model: &str, effort: &str) -> Value {
    json!([
        {"id": "model", "name": "Model", "category": "model", "type": "select",
            "currentValue": model, "options": [
                {"value": "opus", "name": "Opus"}, {"value": "sonnet", "name": "Sonnet"}]},
        {"id": "effort", "name": "Thinking", "category": "thought_level", "type": "select",
            "currentValue": effort, "options": [
                {"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}
    ])
}

fn carried(agent: &str) -> Settings {
    Settings {
        agent: agent.into(),
        auto_accept: true,
        mode: Some("plan".into()),
        options: vec![
            Setting {
                id: "model".into(),
                value: OptionValue::Choice {
                    id: "sonnet".into(),
                },
            },
            Setting {
                id: "effort".into(),
                value: OptionValue::Choice { id: "high".into() },
            },
            Setting {
                id: "gone".into(),
                value: OptionValue::Toggle { value: true },
            },
        ],
    }
}

/// A session given `settings` whose agent opened a new session.
fn opened(settings: Settings, load: Option<&str>) -> Harness {
    let mut harness = harness_with(Config {
        load: load.map(str::to_owned),
        settings: Some(settings),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let method = if load.is_some() {
        "session/load"
    } else {
        "session/new"
    };
    let id = harness.last_request(method)["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"sessionId": "s1", "configOptions": options("opus", "low"),
            "modes": {"currentModeId": "default", "availableModes": [
                {"id": "default", "name": "Default"}, {"id": "plan", "name": "Plan"}]}}),
    );
    harness
}

fn only<'a>(sent: &'a [Value], method: &str) -> Vec<&'a Value> {
    sent.iter()
        .filter(|message| message["method"] == method)
        .collect()
}

#[test]
fn carried_settings_go_one_at_a_time_then_the_mode() {
    let mut harness = opened(carried("claude"), None);
    let sent_now = harness.sent();
    let first = only(&sent_now, "session/set_config_option");
    assert_eq!(first.len(), 1, "one at a time: {sent_now:?}");
    assert_eq!(first[0]["params"]["configId"], "model");
    assert_eq!(first[0]["params"]["value"], "sonnet");
    // The new model resets thinking, which the next setting then sees.
    harness.respond(
        first[0]["id"].as_i64().unwrap(),
        json!({"configOptions": options("sonnet", "low")}),
    );
    let sent_now = harness.sent();
    let second = only(&sent_now, "session/set_config_option");
    assert_eq!(second.len(), 1, "{sent_now:?}");
    assert_eq!(second[0]["params"]["configId"], "effort");
    assert!(only(&sent_now, "session/set_mode").is_empty());
    harness.respond(
        second[0]["id"].as_i64().unwrap(),
        json!({"configOptions": options("sonnet", "high")}),
    );
    // A setting the agent does not offer is skipped; the mode goes last.
    let sent_now = harness.sent();
    assert!(only(&sent_now, "session/set_config_option").is_empty());
    let mode = only(&sent_now, "session/set_mode");
    assert_eq!(mode.len(), 1, "{sent_now:?}");
    assert_eq!(mode[0]["params"]["modeId"], "plan");
    assert!(harness.transcript().auto_accept);
    harness.respond(mode[0]["id"].as_i64().unwrap(), json!({}));
    assert_eq!(harness.transcript().modes.as_ref().unwrap().current, "plan");
    assert!(harness.sent().is_empty());
    assert!(harness.notices().is_empty(), "{:?}", harness.notices());
}

#[test]
fn a_refused_setting_does_not_stop_the_rest() {
    let mut harness = opened(carried("claude"), None);
    let sent_now = harness.sent();
    let first = only(&sent_now, "session/set_config_option");
    harness.fail(
        first[0]["id"].as_i64().unwrap(),
        wire::Error::invalid_params(),
    );
    let sent_now = harness.sent();
    let next = only(&sent_now, "session/set_config_option");
    assert_eq!(next.len(), 1, "{sent_now:?}");
    assert_eq!(next[0]["params"]["configId"], "effort");
}

#[test]
fn settings_of_another_agent_or_for_a_continued_session_stay_unused() {
    for (agent, load) in [("codex", None), ("claude", Some("old"))] {
        let harness = opened(carried(agent), load);
        let sent_now = harness.sent();
        assert!(only(&sent_now, "session/set_config_option").is_empty());
        assert!(only(&sent_now, "session/set_mode").is_empty());
        assert!(!harness.transcript().auto_accept);
    }
}

#[test]
fn a_prompt_waits_until_every_carried_setting_is_set() {
    let mut harness = opened(carried("claude"), None);
    let mut sent_now = harness.sent();
    harness.command(
        1,
        Command::Prompt {
            origin: None,
            text: "go".into(),
            attachments: Vec::new(),
        },
    );
    assert_eq!(harness.transcript().queued.len(), 1);
    for configs in [options("sonnet", "low"), options("sonnet", "high")] {
        sent_now.extend(harness.sent());
        assert!(only(&sent_now, "session/prompt").is_empty(), "{sent_now:?}");
        let set = only(&sent_now, "session/set_config_option");
        let id = set.last().unwrap()["id"].as_i64().unwrap();
        harness.respond(id, json!({ "configOptions": configs }));
        sent_now.clear();
    }
    sent_now = harness.sent();
    assert!(only(&sent_now, "session/prompt").is_empty(), "{sent_now:?}");
    let mode = only(&sent_now, "session/set_mode")[0]["id"]
        .as_i64()
        .unwrap();
    harness.respond(mode, json!({}));
    let sent_now = harness.sent();
    let prompts = only(&sent_now, "session/prompt");
    assert_eq!(prompts.len(), 1, "{sent_now:?}");
    assert!(harness.transcript().queued.is_empty());
}

#[test]
fn a_prompt_goes_at_once_when_nothing_is_carried() {
    for (agent, load) in [("codex", None), ("claude", Some("old"))] {
        let mut harness = opened(carried(agent), load);
        harness.sent();
        harness.command(
            1,
            Command::Prompt {
                origin: None,
                text: "go".into(),
                attachments: Vec::new(),
            },
        );
        let sent_now = harness.sent();
        assert_eq!(only(&sent_now, "session/prompt").len(), 1, "{sent_now:?}");
    }
}
