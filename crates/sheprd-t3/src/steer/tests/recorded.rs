//! The commands a recorded session sent a T3 Code 0.1.9 server, rebuilt
//! from the thread as it stood when each was sent.

use super::*;
use crate::fixtures::{STEER, frames};

/// The chat command id a recorded `commandId` came from.
fn chat_id(payload: &Value) -> CommandId {
    let command = payload["commandId"].as_str().unwrap();
    CommandId(command.strip_prefix("sheprd:").unwrap().parse().unwrap())
}

/// The chat command a recorded dispatch stands for, given the thread then.
fn command_for(thread: &Thread, payload: &Value) -> Command {
    let choice = |id: &str, value: &str| Command::SetOption {
        id: id.to_owned(),
        value: OptionValue::Choice {
            id: value.to_owned(),
        },
    };
    match payload["type"].as_str().unwrap() {
        "message.dispatch" => Command::Prompt {
            text: payload["text"].as_str().unwrap().to_owned(),
            attachments: Vec::new(),
            origin: None,
        },
        "runtime-request.respond" => {
            let permissions = crate::transcript::permissions(thread);
            let [permission] = permissions.as_slice() else {
                panic!("one waiting request, not {permissions:?}");
            };
            Command::Answer {
                option: payload["decision"].as_str().unwrap().to_owned(),
                permission: permission.id,
            }
        }
        "run.interrupt" => Command::Cancel,
        "thread.model-selection.set" => {
            choice(MODEL, payload["modelSelection"]["model"].as_str().unwrap())
        }
        "provider.switch" => choice(
            PROVIDER,
            payload["modelSelection"]["instanceId"].as_str().unwrap(),
        ),
        other => panic!("not a command a chat sends: {other}"),
    }
}

#[test]
fn every_recorded_command_is_the_one_a_chat_command_makes() {
    let mut thread = Thread::default();
    let mut providers = Vec::new();
    let mut checked = Vec::new();
    let mut answered = false;
    for frame in frames(STEER) {
        let request = frame["requestId"].as_str().or(frame["id"].as_str());
        match (
            frame["dir"].as_str().unwrap(),
            frame["_tag"].as_str().unwrap(),
        ) {
            ("server", "Exit") if request == Some("1") => {
                providers = crate::steer::providers(&frame["exit"]["value"]);
            }
            ("server", "Chunk") if request == Some("3") => {
                for value in frame["values"].as_array().unwrap() {
                    thread.apply(value.clone());
                }
            }
            ("client", "Request") if frame["tag"] == DISPATCH => {
                let payload = &frame["payload"];
                // The recorder set the thread to ask before the steps.
                if payload["type"] == "thread.runtime-mode.set" {
                    continue;
                }
                // Request 7 asks about a request that never existed, to
                // record T3's refusal; a chat refuses it before sending.
                if payload["requestId"] == "runtime-request:unknown" {
                    assert!(crate::transcript::permissions(&thread).is_empty());
                    continue;
                }
                answered |= payload["type"] == "runtime-request.respond";
                let command = command_for(&thread, payload);
                let built = dispatch(&thread, &providers, chat_id(payload), &command).unwrap();
                assert_eq!(&serde_json::to_value(built).unwrap(), payload);
                checked.push(payload["type"].as_str().unwrap().to_owned());
            }
            _ => {}
        }
    }
    assert!(answered);
    // The prompt, its approval, the same prompt again, a slow prompt, one
    // queued behind it, its stop, then a model and a provider change.
    assert_eq!(
        checked,
        [
            "message.dispatch",
            "runtime-request.respond",
            "message.dispatch",
            "message.dispatch",
            "message.dispatch",
            "run.interrupt",
            "thread.model-selection.set",
            "provider.switch",
        ]
    );
    // The thread follows the switch it was sent.
    assert_eq!(
        thread.header.model_selection,
        Some(ModelSelection {
            instance_id: "fakeacp2".to_owned(),
            model: "fast".to_owned(),
        })
    );
}

#[test]
fn only_providers_that_can_run_a_thread_are_offered() {
    let config = frames(STEER)
        .into_iter()
        .find(|frame| frame["_tag"] == "Exit" && frame["requestId"] == "1")
        .unwrap();
    let offered: Vec<String> = crate::steer::providers(&config["exit"]["value"])
        .into_iter()
        .map(|provider| provider.instance_id)
        .collect();
    assert_eq!(offered, ["fakeacp", "fakeacp2"]);
}

#[test]
fn a_refused_command_keeps_t3s_reason() {
    let refused = frames(STEER)
        .into_iter()
        .find(|frame| frame["_tag"] == "Exit" && frame["requestId"] == "7")
        .unwrap();
    let frame: crate::frame::ServerFrame = serde_json::from_value(refused).unwrap();
    let crate::frame::ServerFrame::Exit { exit, .. } = frame else {
        panic!("an exit");
    };
    assert!(
        exit.cause().unwrap().contains("was not found"),
        "{:?}",
        exit.cause()
    );
}
