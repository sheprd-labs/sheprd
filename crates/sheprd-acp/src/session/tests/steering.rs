use super::*;
use crate::harness::steer;

/// A session whose agent steers, as Sheprd's own translators do.
fn steering() -> Harness {
    let mut harness = harness_with(config());
    harness.session.start();
    let initialize = harness.last_request("initialize");
    harness.respond(
        initialize["id"].as_i64().unwrap(),
        json!({"protocolVersion": 1, "agentCapabilities": {"_meta": {steer::CAPABILITY: true}}}),
    );
    let new = harness.last_request("session/new");
    harness.respond(new["id"].as_i64().unwrap(), json!({"sessionId": "s1"}));
    harness
}

fn typed(harness: &mut Harness, text: &str) -> CommandId {
    harness.command(
        0,
        Command::Prompt {
            origin: None,
            text: text.to_owned(),
            attachments: Vec::new(),
        },
    )
}

fn notify(harness: &mut Harness, method: &str, id: CommandId) {
    harness.input(Input::Agent(
        HarnessId::default(),
        Inbound::Notification {
            method: method.to_owned(),
            params: json!({"sessionId": "s1", "id": id.0}),
        },
    ));
}

fn users(harness: &Harness) -> Vec<String> {
    harness
        .transcript()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_prompt_typed_during_a_turn_goes_into_it_and_shows_once_taken() {
    let mut harness = steering();
    prompt(&mut harness, 0, "one");
    harness.agent_says("Working on it.");
    let id = typed(&mut harness, "also check the docs");

    let request = harness.last_request(steer::METHOD);
    assert_eq!(request["params"]["id"], json!(id.0));
    assert_eq!(request["params"]["sessionId"], "s1");
    let texts: Vec<&str> = request["params"]["prompt"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect();
    assert_eq!(texts.last(), Some(&"also check the docs"));
    let queued = &harness.transcript().queued;
    assert_eq!(queued.len(), 1);
    assert!(queued[0].steered);
    assert_eq!(users(&harness), ["one"]);

    // Withdrawing it is refused: the agent already has it.
    harness.command(0, Command::Unqueue { id });
    assert_eq!(harness.transcript().queued.len(), 1);

    harness.respond(request["id"].as_i64().unwrap(), json!({}));
    notify(&mut harness, steer::STEERED, id);
    assert!(harness.transcript().queued.is_empty());
    assert_eq!(users(&harness), ["one", "also check the docs"]);
    assert_eq!(
        harness.prompts.borrow().last().unwrap(),
        "also check the docs"
    );
    // The steered prompt started no turn of its own.
    assert!(
        harness
            .sent()
            .iter()
            .all(|message| message["method"] != "session/prompt")
    );
}

#[test]
fn a_steer_the_agent_refuses_waits_for_the_next_turn() {
    let mut harness = steering();
    let first = prompt(&mut harness, 0, "one");
    typed(&mut harness, "two");
    let request = harness.last_request(steer::METHOD);
    harness.fail(
        request["id"].as_i64().unwrap(),
        wire::Error::new(-32603, "No turn is running."),
    );
    let queued = &harness.transcript().queued;
    assert_eq!(queued.len(), 1);
    assert!(!queued[0].steered);
    // Prompts after it queue behind it rather than overtake it.
    typed(&mut harness, "three");
    assert!(
        harness
            .sent()
            .iter()
            .all(|message| message["method"] != steer::METHOD)
    );

    harness.respond(first, json!({"stopReason": "end_turn"}));
    let next = harness.last_request("session/prompt");
    assert_eq!(next["params"]["prompt"][0]["text"], "two");
    assert_eq!(users(&harness), ["one", "two"]);
}

#[test]
fn a_steer_given_back_goes_next_as_it_was_sent() {
    let mut harness = steering();
    let first = prompt(&mut harness, 0, "one");
    let id = typed(&mut harness, "two");
    let request = harness.last_request(steer::METHOD);
    harness.respond(request["id"].as_i64().unwrap(), json!({}));
    notify(&mut harness, steer::UNSTEERED, id);
    assert!(!harness.transcript().queued[0].steered);

    harness.respond(first, json!({"stopReason": "end_turn"}));
    let next = harness.last_request("session/prompt");
    assert_eq!(next["params"]["prompt"], request["params"]["prompt"]);
    assert!(harness.transcript().queued.is_empty());
}

#[test]
fn a_turn_that_ends_without_a_steer_sends_it_next() {
    let mut harness = steering();
    let first = prompt(&mut harness, 0, "one");
    typed(&mut harness, "two");
    let request = harness.last_request(steer::METHOD);
    harness.respond(request["id"].as_i64().unwrap(), json!({}));
    harness.respond(first, json!({"stopReason": "end_turn"}));
    let next = harness.last_request("session/prompt");
    assert_eq!(next["params"]["prompt"], request["params"]["prompt"]);
    assert_eq!(users(&harness), ["one", "two"]);
}

#[test]
fn cancelling_holds_steered_prompts_unless_the_agent_took_them() {
    let mut harness = steering();
    prompt(&mut harness, 0, "one");
    let id = typed(&mut harness, "two");
    harness.command(0, Command::Cancel);
    let queued = &harness.transcript().queued;
    assert_eq!(queued.len(), 1);
    assert!(queued[0].held && !queued[0].steered);
    // The agent took it before the stop reached it: it shows, once.
    notify(&mut harness, steer::STEERED, id);
    assert!(harness.transcript().queued.is_empty());
    assert_eq!(users(&harness), ["one", "two"]);
}

#[test]
fn an_agent_that_cannot_steer_queues_as_before() {
    let mut harness = ready();
    prompt(&mut harness, 0, "one");
    typed(&mut harness, "two");
    assert!(
        harness
            .sent()
            .iter()
            .all(|message| message["method"] != steer::METHOD)
    );
    assert!(!harness.transcript().queued[0].steered);
}

#[test]
fn withdrawing_a_refused_steer_keeps_nothing_it_was_prepared_with() {
    let mut harness = steering();
    prompt(&mut harness, 0, "one");
    let id = typed(&mut harness, "two");
    let request = harness.last_request(steer::METHOD);
    harness.fail(
        request["id"].as_i64().unwrap(),
        wire::Error::new(-32603, "No turn is running."),
    );
    assert!(harness.session.steering.contains_key(&id));
    harness.command(0, Command::Unqueue { id });
    assert!(harness.transcript().queued.is_empty());
    assert!(harness.session.steering.is_empty());
}

#[test]
fn a_steer_given_back_goes_with_its_files_as_they_were_sent() {
    let mut harness = harness_with(config());
    harness.session.start();
    let initialize = harness.last_request("initialize");
    harness.respond(
        initialize["id"].as_i64().unwrap(),
        json!({"protocolVersion": 1, "agentCapabilities": {
            "promptCapabilities": {"embeddedContext": true},
            "_meta": {steer::CAPABILITY: true},
        }}),
    );
    let new = harness.last_request("session/new");
    harness.respond(new["id"].as_i64().unwrap(), json!({"sessionId": "s1"}));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "as sent").unwrap();

    let first = prompt(&mut harness, 0, "one");
    let id = harness.command(
        0,
        Command::Prompt {
            origin: None,
            text: "read this".to_owned(),
            attachments: vec![sheprd_chat_protocol::Attachment {
                path: path.display().to_string(),
            }],
        },
    );
    let request = harness.last_request(steer::METHOD);
    assert!(request.to_string().contains("as sent"));
    harness.respond(request["id"].as_i64().unwrap(), json!({}));
    // The agent changes the file, then the turn ends without the steer.
    std::fs::write(&path, "changed by the agent").unwrap();
    notify(&mut harness, steer::UNSTEERED, id);
    harness.respond(first, json!({"stopReason": "end_turn"}));
    let next = harness.last_request("session/prompt");
    assert_eq!(next["params"]["prompt"], request["params"]["prompt"]);
    assert!(harness.session.steering.is_empty());
}
