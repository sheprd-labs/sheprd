//! A guest stays a guest: across a resend on a new connection, an agent
//! switch, and a replay of the saved conversation.

use sheprd_chat_protocol::{GUEST_ALLOW, GUEST_DENY, Guest, Origin, Rejection};

use super::harnesses::{choosing, launch, requests, respond_second, set_agent};
use super::*;
use crate::context::{Facts, Prompts};

const PERSON: u64 = 1;
const RELAY: u64 = 2;

fn bob() -> Guest {
    Guest::new("Bob", "tma")
}

fn guest_prompt(text: &str) -> Command {
    Command::GuestPrompt {
        text: text.to_owned(),
        guest: bob(),
    }
}

fn acks(receiver: &Receiver<ServerMessage>, id: CommandId) -> Vec<Option<Rejection>> {
    drain(receiver)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::Ack {
                id: acked,
                rejected,
            } if acked == id => Some(rejected),
            _ => None,
        })
        .collect()
}

fn answer(harness: &mut Harness, option: &str) {
    let permission = harness.transcript().permissions.last().unwrap().id;
    harness.command(
        PERSON,
        Command::Answer {
            option: option.to_owned(),
            permission,
        },
    );
}

#[test]
fn a_connection_resending_a_decided_guest_prompt_is_a_guest_connection() {
    let mut harness = ready();
    let _relay = harness.subscribe(RELAY, false, None);
    let id = harness.command(RELAY, guest_prompt("hello"));
    answer(&mut harness, GUEST_DENY);
    harness.input(Input::Detach(ConnId(RELAY)));

    const RETRY: u64 = 3;
    let retry = harness.subscribe(RETRY, false, None);
    harness.input(Input::Command {
        command: guest_prompt("hello"),
        conn: ConnId(RETRY),
        id,
    });
    assert_eq!(acks(&retry, id), [Some(Rejection::Denied)]);
    let auto = harness.command(RETRY, Command::SetAutoAccept { on: true });
    assert_eq!(acks(&retry, auto), [Some(Rejection::GuestOnly)]);
    assert!(!harness.transcript().auto_accept);
}

#[test]
fn a_switch_keeps_waiting_guests_and_the_new_agent_hears_them_as_guests() {
    let mut harness = choosing();
    let relay = harness.subscribe(RELAY, false, None);
    let id = harness.command(RELAY, guest_prompt("add a test"));
    set_agent(&mut harness, Agent::Codex);
    let waiting = &harness.transcript().permissions;
    assert_eq!(waiting.len(), 1, "the person can still answer it");
    assert!(waiting[0].guest.is_some());

    // Allowed before the new agent runs, it waits in the queue; as a
    // steered prompt does, it keeps no origin of its own there.
    answer(&mut harness, GUEST_ALLOW);
    assert_eq!(acks(&relay, id), [None]);
    harness.session.queued_origins.clear();

    let agent = launch(&mut harness);
    let initialize = requests(&agent)[0]["id"].as_i64().unwrap();
    respond_second(
        &mut harness,
        initialize,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let new = requests(&agent)[0]["id"].as_i64().unwrap();
    respond_second(&mut harness, new, json!({"sessionId": "c1"}));
    let sent = requests(&agent);
    assert_eq!(sent[0]["method"], "session/prompt");
    let blocks = sent[0]["params"]["prompt"].as_array().unwrap();
    assert!(
        blocks.iter().any(|block| block["text"]
            .as_str()
            .is_some_and(|text| text.starts_with("<sheprd guest=\"Bob\" tool=\"tma\""))),
        "{blocks:?}"
    );
}

#[test]
fn a_replayed_guest_prompt_is_named_again() {
    let mut harness = harness_with(Config {
        load: Some("old".to_owned()),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load");
    let note = Context::new(Prompts::default(), Facts::default(), Default::default())
        .before(Some(Origin::guest(bob())))
        .unwrap();
    for text in [note.as_str(), "hi from Bob"] {
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
    let users: Vec<_> = harness
        .transcript()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { guest, text, .. } => Some((guest.clone(), text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(users, [(Some(bob()), "hi from Bob".to_owned())]);
}
