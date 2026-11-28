//! What Sheprd tells the agent besides the person's words: the system
//! prompt and where each prompt was typed.

use sheprd_chat_protocol::{Device, Origin, Via};

use crate::context::{Facts, Prompts};
use crate::session::tests::*;
use sheprd_chat_protocol::{Entry, EntryId, Transcript};

const PHONE: Origin = Origin::new(Device::PhoneApp, Via::Tailscale);
const DESKTOP: Origin = Origin::new(Device::Desktop, Via::Local);

fn from(origin: Option<Origin>, text: &str) -> Command {
    Command::Prompt {
        origin,
        text: text.to_owned(),
        attachments: Vec::new(),
    }
}

/// The last `session/prompt`: its id and its text blocks.
fn sent_prompt(harness: &Harness) -> (i64, Vec<String>) {
    let request = harness.last_request("session/prompt");
    let blocks = request["params"]["prompt"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["text"].as_str().unwrap().to_owned())
        .collect();
    (request["id"].as_i64().unwrap(), blocks)
}

fn finish(harness: &mut Harness, id: i64) {
    harness.respond(id, json!({"stopReason": "end_turn"}));
}

#[test]
fn a_phone_prompt_tells_the_agent_first_and_the_transcript_shows_only_the_words() {
    let mut harness = ready();
    harness.command(0, from(Some(PHONE), "show me the mockups"));
    let (id, sent) = sent_prompt(&harness);
    assert_eq!(sent.len(), 2);
    assert!(sent[0].starts_with("<sheprd device=\"phone_app\""));
    assert!(sent[0].contains("Tailscale"));
    assert!(sent[0].contains("Markdown image"));
    assert_eq!(sent[1], "show me the mockups");
    let shown: Vec<_> = harness
        .transcript()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(shown, ["show me the mockups"]);

    // Each prompt identifies the current device; only a move repeats its guide.
    finish(&mut harness, id);
    harness.command(0, from(Some(PHONE), "again"));
    let (id, sent) = sent_prompt(&harness);
    assert!(sent[0].contains("This message came from"));
    assert!(!sent[0].contains("Markdown image"));
    assert_eq!(sent[1], "again");
    finish(&mut harness, id);
    harness.command(0, from(Some(DESKTOP), "back at my desk"));
    let (_, sent) = sent_prompt(&harness);
    assert!(sent[0].contains("desktop app on this computer"));
    assert_eq!(sent[1], "back at my desk");
}

#[test]
fn a_prompt_without_an_origin_sends_only_its_words() {
    let mut harness = ready();
    harness.command(0, from(None, "hi"));
    assert_eq!(sent_prompt(&harness).1, ["hi"]);
}

#[test]
fn antigravity_gets_the_persons_prompt_without_sheprd_annotations() {
    let mut harness = harness_with(Config {
        antigravity_auth_method: Some("oauth-personal".to_owned()),
        agent: "Antigravity".to_owned(),
        harness: Some(Agent::Antigravity),
        listens: true,
        prompts: Prompts::with_instructions("Do not inject this into Antigravity."),
        sheprd: Some(PathBuf::from("/bin/sheprd")),
        ..config()
    });
    harness.session.start();
    let initialize = harness.last_request("initialize");
    assert_eq!(initialize["params"]["clientInfo"]["name"], "sheprd");
    let id = initialize["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {}, "authMethods": [
            {"id": "oauth-personal", "name": "Log in with Google"}
        ]}),
    );
    let authenticate = harness.last_request("authenticate");
    assert_eq!(authenticate["params"]["methodId"], "oauth-personal");
    harness.respond(authenticate["id"].as_i64().unwrap(), json!({}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(id, json!({"sessionId": "antigravity-1"}));

    harness.command(0, from(Some(PHONE), "make the change"));
    let (id, sent) = sent_prompt(&harness);
    assert_eq!(sent, ["make the change"]);
    assert!(!sent.iter().any(|text| text.contains("Do not inject")));
    finish(&mut harness, id);
}

#[test]
fn a_queued_prompt_keeps_where_it_was_typed() {
    let mut harness = ready();
    harness.command(0, from(Some(DESKTOP), "one"));
    let (id, _) = sent_prompt(&harness);
    harness.command(0, from(Some(PHONE), "two"));
    finish(&mut harness, id);
    let (_, sent) = sent_prompt(&harness);
    assert!(sent[0].contains("phone"), "{sent:?}");
    assert_eq!(sent[1], "two");
}

#[test]
fn the_system_prompt_goes_with_the_first_prompt_only() {
    let mut harness = harness_with(Config {
        prompts: Prompts::with_instructions("Answer in French."),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(id, json!({"sessionId": "s1"}));
    harness.command(0, from(None, "bonjour"));
    let (id, sent) = sent_prompt(&harness);
    assert!(sent[0].contains("Answer in French."));
    assert_eq!(sent[1], "bonjour");
    finish(&mut harness, id);
    harness.command(0, from(None, "encore"));
    assert_eq!(sent_prompt(&harness).1, ["encore"]);
}

/// The note a chat sent from the phone with the instructions "Be brief.".
fn said_on_phone() -> String {
    Context::new(
        Prompts::with_instructions("Be brief."),
        Facts::default(),
        Default::default(),
    )
    .before(Some(PHONE))
    .unwrap()
}

fn said(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            guest: None,
            text: text.to_owned(),
            files: 0,
            note: None,
        },
    }
}

/// A chat continued from a log that holds a note sent from the phone.
fn continued() -> Harness {
    let mut history = Transcript::new("Claude".to_owned());
    history.entries = vec![
        said(1, &format!("{}\nshow me the mockups", said_on_phone())),
        said(2, &said_on_phone()),
    ];
    let mut harness = harness_with(Config {
        history: Some(history),
        prompts: Prompts::with_instructions("Be brief."),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(id, json!({"sessionId": "s1"}));
    harness
}

#[test]
fn a_restored_chat_keeps_its_notes_apart_from_the_words() {
    let harness = continued();
    let users: Vec<_> = harness
        .transcript()
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::User { .. }))
        .collect();
    let mut expected = said(1, "show me the mockups");
    if let EntryKind::User { note, .. } = &mut expected.kind {
        *note = Some(said_on_phone());
    }
    assert_eq!(users, [&expected]);
}

#[test]
fn a_restored_chat_tells_again_only_on_another_device() {
    let mut harness = continued();
    harness.command(0, from(Some(PHONE), "and the dark variant"));
    let (id, sent) = sent_prompt(&harness);
    assert!(!sent[0].contains("Markdown image"));
    assert_eq!(sent[1], "and the dark variant");
    finish(&mut harness, id);
    harness.command(0, from(Some(DESKTOP), "at my desk now"));
    let (_, sent) = sent_prompt(&harness);
    assert!(sent[0].starts_with("<sheprd device=\"desktop\" paths=\"here\""));
    assert!(!sent[0].contains("Be brief."), "{}", sent[0]);
}

#[test]
fn a_replayed_conversation_hides_its_notes_and_counts_them_told() {
    let mut harness = harness_with(Config {
        load: Some("old".to_owned()),
        prompts: Prompts::with_instructions("Be brief."),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load");
    for text in [said_on_phone().as_str(), "earlier"] {
        harness.input(Input::Agent(
            HarnessId::default(),
            Inbound::Notification {
                method: "session/update".to_owned(),
                params: json!({"sessionId": "old", "update": {"sessionUpdate": "user_message_chunk",
                "content": {"type": "text", "text": text}}}),
            },
        ));
    }
    harness.respond(load["id"].as_i64().unwrap(), json!({}));
    // The notice under the replayed conversation is the chat's own.
    let texts: Vec<_> = harness
        .transcript()
        .entries
        .iter()
        .filter(|entry| !matches!(entry.kind, EntryKind::Notice { .. }))
        .filter_map(|entry| entry.kind.text())
        .collect();
    assert_eq!(texts, ["earlier"]);
    harness.command(0, from(Some(PHONE), "hi"));
    let (_, sent) = sent_prompt(&harness);
    assert!(!sent[0].contains("Be brief."));
    assert!(sent[0].contains("This message came from"));
    assert_eq!(sent[1], "hi");
}

#[test]
fn queued_messages_keep_their_own_phone_target() {
    let mut harness = ready();
    let mut first = PHONE;
    first.phone = Some("phone-a".into());
    let mut second = PHONE;
    second.phone = Some("phone-b".into());
    harness.command(0, from(Some(first), "one"));
    let (id, sent) = sent_prompt(&harness);
    assert!(sent[0].contains("--device phone-a"));
    harness.command(0, from(Some(second), "two"));
    harness.command(0, from(Some(DESKTOP), "three"));
    finish(&mut harness, id);
    let (id, sent) = sent_prompt(&harness);
    assert!(sent[0].contains("--device phone-b"));
    assert!(!sent[0].contains("phone-a"));
    assert_eq!(sent[1], "two");
    finish(&mut harness, id);
    let (_, sent) = sent_prompt(&harness);
    assert!(sent[0].contains("No phone target is identified"));
    assert_eq!(sent[1], "three");
}
