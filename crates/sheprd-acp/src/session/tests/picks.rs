//! The model and thinking picked last for each agent, kept across chats.

use std::path::Path;

use sheprd_chat_protocol::{OptionValue, Setting, Settings};

use super::harnesses::{choosing, launch, requests, set_agent};
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

fn choice(id: &str, value: &str) -> Setting {
    Setting {
        id: id.into(),
        value: OptionValue::Choice { id: value.into() },
    }
}

fn picked(agent: &str, model: &str, effort: &str) -> Settings {
    Settings {
        agent: agent.into(),
        options: vec![choice("model", model), choice("effort", effort)],
        ..Settings::default()
    }
}

/// A chat keeping picks in `dir` whose agent opened session `s1`, offering
/// Opus with low thinking and two modes.
fn opened(dir: &Path, config: Config) -> Harness {
    let load = config.load.is_some();
    let mut harness = harness_with(config);
    harness.session.keep_picks(dir.to_owned());
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let method = if load { "session/load" } else { "session/new" };
    let id = harness.last_request(method)["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"sessionId": "s1", "configOptions": options("opus", "low"),
            "modes": {"currentModeId": "default", "availableModes": [
                {"id": "default", "name": "Default"}, {"id": "plan", "name": "Plan"}]}}),
    );
    harness
}

fn sent_options(harness: &Harness) -> Vec<(String, String)> {
    harness
        .sent()
        .iter()
        .filter(|message| message["method"] == "session/set_config_option")
        .map(|message| {
            (
                message["params"]["configId"].as_str().unwrap().to_owned(),
                message["params"]["value"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_pick_is_kept_without_the_mode_or_auto_accepting() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = opened(root.path(), config());
    harness.command(0, Command::SetAutoAccept { on: true });
    harness.command(
        0,
        Command::SetOption {
            id: "effort".into(),
            value: OptionValue::Choice { id: "high".into() },
        },
    );
    let request = harness.last_request("session/set_config_option");
    harness.respond(
        request["id"].as_i64().unwrap(),
        json!({"configOptions": options("opus", "high")}),
    );
    assert_eq!(
        crate::known::picked(root.path(), Agent::Claude),
        Some(picked("claude", "opus", "high"))
    );
    assert_eq!(crate::known::picked(root.path(), Agent::Codex), None);
}

#[test]
fn a_new_chat_sets_the_agents_picks_once_its_session_opens() {
    let root = tempfile::tempdir().unwrap();
    crate::known::save_picked(
        root.path(),
        Agent::Claude,
        &picked("claude", "opus", "high"),
    )
    .unwrap();
    let mut harness = opened(root.path(), config());
    // Opus is already set, so only the thinking goes.
    let request = harness.last_request("session/set_config_option");
    assert_eq!(request["params"]["configId"], "effort");
    assert_eq!(request["params"]["value"], "high");
    harness.respond(
        request["id"].as_i64().unwrap(),
        json!({"configOptions": options("opus", "high")}),
    );
    assert!(sent_options(&harness).is_empty());
    assert!(harness.notices().is_empty(), "{:?}", harness.notices());
}

#[test]
fn a_continued_chat_or_one_given_settings_keeps_them_over_the_picks() {
    let root = tempfile::tempdir().unwrap();
    crate::known::save_picked(
        root.path(),
        Agent::Claude,
        &picked("claude", "opus", "high"),
    )
    .unwrap();
    let continued = opened(
        root.path(),
        Config {
            load: Some("old".into()),
            ..config()
        },
    );
    assert!(sent_options(&continued).is_empty());
    let given = opened(
        root.path(),
        Config {
            settings: Some(picked("claude", "sonnet", "low")),
            ..config()
        },
    );
    assert_eq!(sent_options(&given), [("model".into(), "sonnet".into())]);
}

#[test]
fn switching_agents_brings_back_the_new_agents_picks() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = opened(
        root.path(),
        Config {
            choices: Some(vec![Agent::Claude, Agent::Codex]),
            ..config()
        },
    );
    // Picked in another chat after this one started.
    crate::known::save_picked(
        root.path(),
        Agent::Codex,
        &picked("codex", "sonnet", "high"),
    )
    .unwrap();
    set_agent(&mut harness, Agent::Codex);
    let agent = launch(&mut harness);
    let id = requests(&agent)[0]["id"].as_i64().unwrap();
    answer(
        &mut harness,
        1,
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let id = requests(&agent)[0]["id"].as_i64().unwrap();
    answer(
        &mut harness,
        1,
        id,
        json!({"sessionId": "c1", "configOptions": options("opus", "low")}),
    );
    let sent = requests(&agent);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["method"], "session/set_config_option");
    assert_eq!(sent[0]["params"]["configId"], "model");
    assert_eq!(sent[0]["params"]["value"], "sonnet");
}

#[test]
fn switching_back_to_a_resumed_session_keeps_what_it_ran_with() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = choosing();
    harness.session.keep_picks(root.path().to_owned());
    let turn = prompt(&mut harness, 1, "hello");
    harness.respond(turn, json!({"stopReason": "end_turn"}));
    set_agent(&mut harness, Agent::Codex);
    let codex = launch(&mut harness);
    let id = requests(&codex)[0]["id"].as_i64().unwrap();
    answer(
        &mut harness,
        1,
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let id = requests(&codex)[0]["id"].as_i64().unwrap();
    answer(&mut harness, 1, id, json!({"sessionId": "c1"}));
    // Picked in another chat while this one ran Codex.
    crate::known::save_picked(
        root.path(),
        Agent::Claude,
        &picked("claude", "sonnet", "high"),
    )
    .unwrap();
    set_agent(&mut harness, Agent::Claude);
    let claude = launch(&mut harness);
    let id = requests(&claude)[0]["id"].as_i64().unwrap();
    answer(
        &mut harness,
        2,
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let open = requests(&claude).remove(0);
    assert_eq!(open["method"], "session/load");
    // Nothing shows the other chat's picks while it resumes.
    assert_eq!(
        harness
            .transcript()
            .options
            .iter()
            .find(|option| option.id == "model"),
        None
    );
    answer(
        &mut harness,
        2,
        open["id"].as_i64().unwrap(),
        json!({"configOptions": options("opus", "low")}),
    );
    let sent = requests(&claude);
    assert!(
        sent.iter()
            .all(|message| message["method"] != "session/set_config_option"),
        "{sent:?}"
    );
}

#[test]
fn a_pick_made_while_starting_is_kept_even_when_the_session_had_it() {
    let root = tempfile::tempdir().unwrap();
    crate::known::save_picked(
        root.path(),
        Agent::Claude,
        &picked("claude", "opus", "high"),
    )
    .unwrap();
    let known = crate::known::Settings {
        modes: None,
        options: crate::project::options(Some(&options("opus", "low"))),
    };
    let mut harness = harness_with(Config {
        known: [(Agent::Claude, known)].into(),
        ..config()
    });
    harness.session.keep_picks(root.path().to_owned());
    harness.session.start();
    harness.command(
        0,
        Command::SetOption {
            id: "effort".into(),
            value: OptionValue::Choice { id: "low".into() },
        },
    );
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    // The new session already runs low thinking: nothing needs setting.
    harness.respond(
        id,
        json!({"sessionId": "s1", "configOptions": options("opus", "low")}),
    );
    assert!(sent_options(&harness).is_empty());
    assert_eq!(
        crate::known::picked(root.path(), Agent::Claude),
        Some(picked("claude", "opus", "low"))
    );
}

/// Answers request `id` as the `agent`-th agent the chat started.
fn answer(harness: &mut Harness, agent: u64, id: i64, result: Value) {
    harness.input(Input::Agent(
        HarnessId(agent),
        Inbound::Response {
            id: wire::RequestId::Number(id),
            result: Ok(result),
        },
    ));
}

#[test]
fn a_mode_picked_while_a_continued_chat_starts_leaves_the_picks() {
    let root = tempfile::tempdir().unwrap();
    crate::known::save_picked(
        root.path(),
        Agent::Claude,
        &picked("claude", "sonnet", "high"),
    )
    .unwrap();
    let known = crate::known::Settings {
        modes: Some(sheprd_chat_protocol::Modes {
            available: ["default", "plan"]
                .map(|id| sheprd_chat_protocol::Mode {
                    description: None,
                    id: id.into(),
                    name: id.into(),
                })
                .into(),
            current: "default".into(),
        }),
        options: crate::project::options(Some(&options("opus", "low"))),
    };
    let mut harness = harness_with(Config {
        known: [(Agent::Claude, known)].into(),
        load: Some("old".into()),
        ..config()
    });
    harness.session.keep_picks(root.path().to_owned());
    harness.session.start();
    harness.command(0, Command::SetMode { id: "plan".into() });
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let id = harness.last_request("session/load")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"configOptions": options("opus", "low"),
            "modes": {"currentModeId": "default", "availableModes": [
                {"id": "default", "name": "Default"}, {"id": "plan", "name": "Plan"}]}}),
    );
    let mode = harness.last_request("session/set_mode");
    harness.respond(mode["id"].as_i64().unwrap(), json!({}));
    assert_eq!(harness.transcript().modes.as_ref().unwrap().current, "plan");
    assert_eq!(
        crate::known::picked(root.path(), Agent::Claude),
        Some(picked("claude", "sonnet", "high"))
    );
}
