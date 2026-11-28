//! The notice under the conversation a continued session brought, which
//! clients fold the earlier turns above.

use super::{
    harnesses::{launch, requests, set_agent},
    *,
};
use sheprd_chat_protocol::{Entry, EntryId, EntryKind, Transcript};

fn history() -> Transcript {
    let mut transcript = Transcript::new("Claude".to_owned());
    transcript.entries = vec![
        Entry {
            id: EntryId(4096),
            kind: EntryKind::User {
                guest: None,
                files: 0,
                note: None,
                text: "Fix the build".to_owned(),
            },
        },
        Entry {
            id: EntryId(8192),
            kind: EntryKind::Agent {
                text: "Fixed.".to_owned(),
            },
        },
    ];
    transcript
}

fn open(config: Config, method: &str, capabilities: Value, response: Value) -> Harness {
    let mut harness = harness_with(config);
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": capabilities}),
    );
    let request = harness.last_request(method);
    harness.respond(request["id"].as_i64().unwrap(), response);
    harness
}

fn resumed(history: Option<Transcript>) -> Harness {
    open(
        Config {
            choices: Some(vec![Agent::Claude, Agent::Codex]),
            history,
            load: Some("old".to_owned()),
            require_load: true,
            ..config()
        },
        "session/load",
        json!({"loadSession": true}),
        json!({}),
    )
}

#[test]
fn a_resumed_conversation_ends_in_a_notice_the_session_names() {
    let harness = resumed(Some(history()));
    let transcript = harness.transcript();
    let last = transcript.entries.last().unwrap();
    assert!(matches!(
        &last.kind,
        EntryKind::Notice { level: NoticeLevel::Info, details: None, text, .. }
            if text.starts_with("Continued from")
    ));
    assert_eq!(
        transcript.session.as_ref().unwrap().continued,
        Some(last.id)
    );
    assert!(last.id > EntryId(8192));
}

#[test]
fn a_resumed_session_without_earlier_words_has_no_divider() {
    let harness = resumed(None);
    assert!(harness.transcript().entries.is_empty());
    assert_eq!(
        harness.transcript().session.as_ref().unwrap().continued,
        None
    );
}

#[test]
fn a_new_session_has_no_divider() {
    let harness = open(
        config(),
        "session/new",
        json!({}),
        json!({"sessionId": "fresh"}),
    );
    assert_eq!(
        harness.transcript().session.as_ref().unwrap().continued,
        None
    );
    assert!(harness.transcript().entries.is_empty());
}

/// Answers the agent started second, by its request `method`.
fn answer_second(harness: &mut Harness, agent: &Receiver<Vec<u8>>, method: &str, result: Value) {
    let request = requests(agent)
        .into_iter()
        .rfind(|request| request["method"] == method)
        .unwrap();
    harness.input(Input::Agent(
        HarnessId(1),
        Inbound::Response {
            id: wire::RequestId::Number(request["id"].as_i64().unwrap()),
            result: Ok(result),
        },
    ));
}

#[test]
fn a_switch_of_agent_keeps_the_first_divider_and_places_no_other() {
    let mut harness = resumed(Some(history()));
    let divider = harness.transcript().session.as_ref().unwrap().continued;
    assert!(divider.is_some());
    let (flow, _) = set_agent(&mut harness, Agent::Codex);
    assert!(matches!(flow, Flow::Launch { .. }), "{flow:?}");
    let agent = launch(&mut harness);
    answer_second(
        &mut harness,
        &agent,
        "initialize",
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    answer_second(
        &mut harness,
        &agent,
        "session/new",
        json!({"sessionId": "s2"}),
    );
    let transcript = harness.transcript();
    assert_eq!(transcript.session.as_ref().unwrap().id, "s2");
    assert_eq!(transcript.session.as_ref().unwrap().continued, divider);
    let dividers = transcript
        .entries
        .iter()
        .filter(|entry| {
            matches!(&entry.kind, EntryKind::Notice { text, .. } if text.starts_with("Continued from"))
        })
        .count();
    assert_eq!(dividers, 1);
}

#[test]
fn switching_immediately_after_acp_replay_hands_over_history_and_can_resume_back() {
    use harnesses::{rejection, respond_second};
    use sheprd_chat_protocol::HandedOver;

    let mut harness = harness_with(Config {
        agent: "OpenCode".to_owned(),
        choices: Some(vec![Agent::Opencode, Agent::Claude]),
        harness: Some(Agent::Opencode),
        history: None,
        load: Some("oc-1".to_owned()),
        require_load: false,
        ..config()
    });
    let client = harness.subscribe(1, false, None);
    harness.session.start();
    let init = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        init,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load")["id"].as_i64().unwrap();

    // Switching while the initial load is still replaying is refused.
    drain(&client);
    let (flow, id) = set_agent(&mut harness, Agent::Claude);
    assert_eq!(flow, Flow::Continue);
    assert_eq!(rejection(&drain(&client), id), Some(Rejection::AgentFixed));

    // ACP replays the existing conversation before answering session/load.
    harness.input(Input::Agent(
        HarnessId(0),
        Inbound::Notification {
            method: "session/update".to_owned(),
            params: json!({
                "sessionId": "oc-1",
                "update": {
                    "sessionUpdate": "user_message_chunk",
                    "content": {"type": "text", "text": "Refactor the parser"}
                }
            }),
        },
    ));
    harness.input(Input::Agent(
        HarnessId(0),
        Inbound::Notification {
            method: "session/update".to_owned(),
            params: json!({
                "sessionId": "oc-1",
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": "Refactored parser.rs."}
                }
            }),
        },
    ));
    harness.respond(load, json!({}));

    // Switch immediately before sending another prompt.
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
    answer_second(
        &mut harness,
        &claude,
        "initialize",
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    answer_second(
        &mut harness,
        &claude,
        "session/new",
        json!({"sessionId": "cl-1"}),
    );
    assert_eq!(
        harness.handoffs(),
        [HandedOver {
            from: Some(Agent::Opencode),
            kept: 2,
            omitted: 0,
            resumed: false,
            to: Agent::Claude,
        }]
    );

    // The first prompt to Claude carries the replayed OpenCode conversation.
    harness.command(
        1,
        Command::Prompt {
            origin: None,
            text: "Add tests".to_owned(),
            attachments: Vec::new(),
        },
    );
    let sent = requests(&claude);
    assert_eq!(sent[0]["method"], "session/prompt");
    let blocks = sent[0]["params"]["prompt"].as_array().unwrap();
    let combined: String = blocks
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(combined.contains("Refactor the parser"), "{combined}");
    assert!(combined.contains("Refactored parser.rs."), "{combined}");
    assert!(combined.contains("Add tests"), "{combined}");
    respond_second(
        &mut harness,
        sent[0]["id"].as_i64().unwrap(),
        json!({"stopReason": "end_turn"}),
    );

    // Switching back resumes the original OpenCode session.
    let (flow, _) = set_agent(&mut harness, Agent::Opencode);
    assert_eq!(
        flow,
        Flow::Launch {
            agent: Agent::Opencode,
            harness: HarnessId(2),
            load: Some("oc-1".to_owned()),
        }
    );
}

#[test]
fn a_resumed_chat_can_switch_away_after_its_agent_exits() {
    use harnesses::rejection;

    let mut harness = resumed(Some(history()));
    let client = harness.subscribe(1, false, None);
    harness.input(Input::AgentExited {
        code: Some(1),
        harness: HarnessId(0),
        stderr: vec!["crashed".to_owned()],
    });
    assert!(matches!(harness.transcript().status, Status::Exited { .. }));

    drain(&client);
    let (flow, id) = set_agent(&mut harness, Agent::Codex);
    assert_eq!(
        flow,
        Flow::Launch {
            agent: Agent::Codex,
            harness: HarnessId(1),
            load: None,
        }
    );
    assert_eq!(rejection(&drain(&client), id), None);
}
