use sheprd_chat_protocol::{Device, HandedOver, Origin, Via};

use super::{
    harnesses::{launch, rejection, requests, set_agent},
    *,
};
use crate::context::Prompts;

const PHONE: Origin = Origin::new(Device::PhoneApp, Via::Local);

fn respond_as(harness: &mut Harness, instance: u64, id: i64, result: Value) {
    harness.input(Input::Agent(
        HarnessId(instance),
        Inbound::Response {
            id: wire::RequestId::Number(id),
            result: Ok(result),
        },
    ));
}

fn say_as(harness: &mut Harness, instance: u64, session: &str, text: &str) {
    harness.input(Input::Agent(
        HarnessId(instance),
        Inbound::Notification {
            method: "session/update".to_owned(),
            params: json!({
                "sessionId": session,
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": text}
                }
            }),
        },
    ));
}

fn open_antigravity(
    harness: &mut Harness,
    instance: u64,
    session_id: &str,
    load_session: bool,
) -> Receiver<Vec<u8>> {
    let agy = launch(harness);
    let init = requests(&agy)[0]["id"].as_i64().unwrap();
    respond_as(
        harness,
        instance,
        init,
        json!({
            "protocolVersion": 1,
            "agentCapabilities": {"loadSession": load_session},
            "authMethods": [{"id": "oauth-personal", "name": "Log in with Google"}]
        }),
    );
    let auth = requests(&agy);
    assert_eq!(auth[0]["method"], "authenticate");
    assert_eq!(auth[0]["params"]["methodId"], "oauth-personal");
    respond_as(
        harness,
        instance,
        auth[0]["id"].as_i64().unwrap(),
        json!({}),
    );
    let open = requests(&agy);
    if load_session {
        assert_eq!(open[0]["method"], "session/load");
        respond_as(
            harness,
            instance,
            open[0]["id"].as_i64().unwrap(),
            json!({}),
        );
    } else {
        assert_eq!(open[0]["method"], "session/new");
        respond_as(
            harness,
            instance,
            open[0]["id"].as_i64().unwrap(),
            json!({"sessionId": session_id}),
        );
    }
    agy
}

#[test]
fn switching_to_antigravity_mid_session_hands_it_the_conversation_without_sheprd_notes() {
    let mut harness = harness_with(Config {
        antigravity_auth_method: Some("oauth-personal".to_owned()),
        choices: Some(vec![Agent::Antigravity, Agent::Claude, Agent::Codex]),
        prompts: Prompts::with_instructions("Do not inject into Antigravity."),
        ..config()
    });
    harness.session.start();
    let init = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(init, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let new = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(new, json!({"sessionId": "claude-1"}));

    let client = harness.subscribe(1, false, None);
    let turn = prompt(&mut harness, 1, "start with claude");
    say_as(&mut harness, 0, "claude-1", "Claude answered.");
    harness.respond(turn, json!({"stopReason": "end_turn"}));

    // Antigravity remains offered after a prompt has been sent.
    assert_eq!(
        harness.transcript().harnesses.as_ref().unwrap().available,
        [Agent::Antigravity, Agent::Claude, Agent::Codex]
    );
    drain(&client);
    let (flow, id) = set_agent(&mut harness, Agent::Antigravity);
    assert_eq!(
        flow,
        Flow::Launch {
            agent: Agent::Antigravity,
            harness: HarnessId(1),
            load: None,
        }
    );
    assert_eq!(rejection(&drain(&client), id), None);

    let agy = open_antigravity(&mut harness, 1, "agy-1", false);
    assert_eq!(
        harness.handoffs(),
        [HandedOver {
            from: Some(Agent::Claude),
            kept: 2,
            omitted: 0,
            resumed: false,
            to: Agent::Antigravity,
        }]
    );

    // Next prompt carries the conversation handoff, without Sheprd's context note.
    harness.command(
        1,
        Command::Prompt {
            origin: Some(PHONE),
            text: "continue with agy".to_owned(),
            attachments: Vec::new(),
        },
    );
    let sent = requests(&agy);
    assert_eq!(sent[0]["method"], "session/prompt");
    let blocks = sent[0]["params"]["prompt"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    let handoff = blocks[0]["text"].as_str().unwrap();
    assert!(handoff.contains("with Claude"), "{handoff}");
    assert!(
        handoff.contains("| User: start with claude\n| Agent: Claude answered.\n"),
        "{handoff}"
    );
    assert!(!handoff.contains("Do not inject into Antigravity."));
    assert_eq!(blocks[1]["text"], "continue with agy");

    say_as(&mut harness, 1, "agy-1", "Antigravity answered.");
    respond_as(
        &mut harness,
        1,
        sent[0]["id"].as_i64().unwrap(),
        json!({"stopReason": "end_turn"}),
    );

    // Subsequent prompts go directly to Antigravity without handoff or Sheprd notes.
    harness.command(
        1,
        Command::Prompt {
            origin: Some(PHONE),
            text: "second agy prompt".to_owned(),
            attachments: Vec::new(),
        },
    );
    let sent = requests(&agy);
    assert_eq!(
        sent[0]["params"]["prompt"],
        json!([{"type": "text", "text": "second agy prompt"}])
    );
    respond_as(
        &mut harness,
        1,
        sent[0]["id"].as_i64().unwrap(),
        json!({"stopReason": "end_turn"}),
    );
}

#[test]
fn switching_from_antigravity_mid_session_hands_conversation_and_restores_context() {
    let mut harness = harness_with(Config {
        agent: "Antigravity".to_owned(),
        antigravity_auth_method: Some("oauth-personal".to_owned()),
        choices: Some(vec![Agent::Antigravity, Agent::Claude]),
        harness: Some(Agent::Antigravity),
        prompts: Prompts::with_instructions("Be concise."),
        ..config()
    });
    harness.session.start();
    let init = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(init, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let new = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(new, json!({"sessionId": "agy-1"}));

    let client = harness.subscribe(1, false, None);
    harness.command(
        1,
        Command::Prompt {
            origin: Some(PHONE),
            text: "hello agy".to_owned(),
            attachments: Vec::new(),
        },
    );
    let first = harness.last_request("session/prompt");
    assert_eq!(
        first["params"]["prompt"],
        json!([{"type": "text", "text": "hello agy"}])
    );
    say_as(&mut harness, 0, "agy-1", "Hi from Antigravity.");
    harness.respond(
        first["id"].as_i64().unwrap(),
        json!({"stopReason": "end_turn"}),
    );

    // Switching away after a prompt stays open and hands Antigravity's turns to Claude.
    assert!(harness.transcript().harnesses.is_some());
    drain(&client);
    let (flow, id) = set_agent(&mut harness, Agent::Claude);
    assert_eq!(
        flow,
        Flow::Launch {
            agent: Agent::Claude,
            harness: HarnessId(1),
            load: None,
        }
    );
    assert_eq!(rejection(&drain(&client), id), None);

    let claude = launch(&mut harness);
    let init = requests(&claude)[0]["id"].as_i64().unwrap();
    respond_as(
        &mut harness,
        1,
        init,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let new = requests(&claude)[0]["id"].as_i64().unwrap();
    respond_as(&mut harness, 1, new, json!({"sessionId": "claude-1"}));

    harness.command(
        1,
        Command::Prompt {
            origin: None,
            text: "take over".to_owned(),
            attachments: Vec::new(),
        },
    );
    let sent = requests(&claude);
    let blocks = sent[0]["params"]["prompt"].as_array().unwrap();
    assert_eq!(blocks.len(), 3);
    assert!(blocks[0]["text"].as_str().unwrap().contains("Be concise."));
    let handoff = blocks[1]["text"].as_str().unwrap();
    assert!(handoff.contains("with Antigravity"), "{handoff}");
    assert!(
        handoff.contains("| User: hello agy\n| Agent: Hi from Antigravity.\n"),
        "{handoff}"
    );
    assert_eq!(blocks[2]["text"], "take over");
    say_as(&mut harness, 1, "claude-1", "Claude took over.");
    respond_as(
        &mut harness,
        1,
        sent[0]["id"].as_i64().unwrap(),
        json!({"stopReason": "end_turn"}),
    );

    // Switching back to Antigravity without session/load starts anew with the whole conversation.
    let (flow, _) = set_agent(&mut harness, Agent::Antigravity);
    assert_eq!(
        flow,
        Flow::Launch {
            agent: Agent::Antigravity,
            harness: HarnessId(2),
            load: Some("agy-1".to_owned()),
        }
    );
    let agy = open_antigravity(&mut harness, 2, "agy-2", false);
    harness.command(
        1,
        Command::Prompt {
            origin: Some(PHONE),
            text: "back to agy".to_owned(),
            attachments: Vec::new(),
        },
    );
    let sent = requests(&agy);
    let blocks = sent[0]["params"]["prompt"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    let handoff = blocks[0]["text"].as_str().unwrap();
    assert!(handoff.contains("taking over a conversation"), "{handoff}");
    assert!(handoff.contains("User: hello agy"), "{handoff}");
    assert!(handoff.contains("User: take over"), "{handoff}");
    assert!(handoff.contains("Agent: Claude took over."), "{handoff}");
    assert_eq!(blocks[1]["text"], "back to agy");
}

#[test]
fn switching_away_from_antigravity_before_first_prompt_restores_sheprd_context() {
    let mut harness = harness_with(Config {
        agent: "Antigravity".to_owned(),
        antigravity_auth_method: Some("oauth-personal".to_owned()),
        choices: Some(vec![Agent::Antigravity, Agent::Claude]),
        harness: Some(Agent::Antigravity),
        prompts: Prompts::with_instructions("Be concise."),
        ..config()
    });
    harness.session.start();
    let init = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(init, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let new = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(new, json!({"sessionId": "agy-1"}));

    let (flow, _) = set_agent(&mut harness, Agent::Claude);
    assert_eq!(
        flow,
        Flow::Launch {
            agent: Agent::Claude,
            harness: HarnessId(1),
            load: None,
        }
    );
    let claude = launch(&mut harness);
    let init = requests(&claude)[0]["id"].as_i64().unwrap();
    respond_as(
        &mut harness,
        1,
        init,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let new = requests(&claude)[0]["id"].as_i64().unwrap();
    respond_as(&mut harness, 1, new, json!({"sessionId": "claude-1"}));

    harness.command(
        1,
        Command::Prompt {
            origin: None,
            text: "hi".to_owned(),
            attachments: Vec::new(),
        },
    );
    let sent = requests(&claude);
    let blocks = sent[0]["params"]["prompt"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0]["text"].as_str().unwrap().contains("Be concise."));
    assert_eq!(blocks[1]["text"], "hi");
}
