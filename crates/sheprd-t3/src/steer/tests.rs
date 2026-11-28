use super::*;
use serde_json::json;
use sheprd_chat_protocol::{Attachment, EntryId, PermissionKind};
use uuid::Uuid;

const THREAD: &str = "thread-1";

/// A thread at sequence 1 with `runs`, `requests`, and `items`, running
/// `fakeacp`'s `default` model.
fn thread(runs: Value, requests: Value, items: Value) -> Thread {
    let mut thread = Thread::default();
    thread.apply(
        json!({"kind": "snapshot", "snapshotSequence": 1, "projection": {
        "thread": {"id": THREAD, "title": "Steer", "providerInstanceId": "fakeacp",
            "modelSelection": {"instanceId": "fakeacp", "model": "default"}},
        "runs": runs, "runtimeRequests": requests, "turnItems": items}}),
    );
    thread
}

fn idle() -> Thread {
    thread(json!([]), json!([]), json!([]))
}

fn running() -> Thread {
    thread(
        json!([
            {"id": "run-1", "ordinal": 1, "status": "completed"},
            {"id": "run-2", "ordinal": 2, "status": "running"},
            {"id": "run-3", "ordinal": 3, "status": "queued"},
        ]),
        json!([]),
        json!([]),
    )
}

/// A thread waiting on an approval (entry 1) and a question (entry 2), and
/// with an answered approval (entry 3) and a pair of questions (entry 4).
fn asking() -> Thread {
    thread(
        json!([{"id": "run-1", "ordinal": 1, "status": "waiting"}]),
        json!([
            {"id": "request-1", "status": "pending"},
            {"id": "request-2", "status": "pending"},
            {"id": "request-3", "status": "resolved"},
            {"id": "request-4", "status": "pending"},
        ]),
        json!([
            {"id": "item-1", "ordinal": 1, "type": "approval_request",
                "requestId": "request-1", "requestKind": "command",
                "prompt": "Run `ls`?",
                "options": [{"decision": "accept", "label": "Yes"},
                    {"decision": "decline", "label": "No"}]},
            {"id": "item-2", "ordinal": 2, "type": "user_input_request",
                "requestId": "request-2",
                "questions": [{"id": "color", "header": "Color", "question": "Which color?",
                    "options": [{"label": "Red", "description": "Warm", "value": "red"},
                        {"label": "Blue", "description": "Cold"}]}]},
            {"id": "item-3", "ordinal": 3, "type": "approval_request",
                "requestId": "request-3", "requestKind": "command"},
            {"id": "item-4", "ordinal": 4, "type": "user_input_request",
                "requestId": "request-4",
                "questions": [
                    {"id": "a", "header": "A", "question": "A?", "options": []},
                    {"id": "b", "header": "B", "question": "B?", "options": []}]},
        ]),
    )
}

fn providers() -> Vec<Provider> {
    providers_of(&json!({"providers": [
        {"instanceId": "fakeacp", "displayName": "Fake", "enabled": true,
            "models": [{"slug": "default", "name": "Default", "isDefault": true},
                {"slug": "fast", "name": "Fast"}]},
        {"instanceId": "other", "enabled": true,
            "models": [{"slug": "small", "name": "Small"},
                {"slug": "large", "name": "Large", "isDefault": true}]},
        {"instanceId": "off", "enabled": false,
            "models": [{"slug": "x", "name": "X"}]},
        {"instanceId": "empty", "enabled": true, "models": []},
        {"no": "instance"},
    ]}))
}

fn providers_of(config: &Value) -> Vec<Provider> {
    super::providers(config)
}

fn id(n: u128) -> CommandId {
    CommandId(Uuid::from_u128(n))
}

fn payload(command: &Command) -> Value {
    serde_json::to_value(dispatch(&running(), &providers(), id(7), command).unwrap()).unwrap()
}

#[test]
fn a_prompt_starts_a_run_or_queues_behind_the_one_running() {
    let prompt = Command::Prompt {
        text: "hello".to_owned(),
        attachments: Vec::new(),
        origin: None,
    };
    let now = serde_json::to_value(dispatch(&idle(), &[], id(1), &prompt).unwrap()).unwrap();
    let command = "sheprd:00000000-0000-0000-0000-000000000001";
    assert_eq!(
        now,
        json!({"type": "message.dispatch", "commandId": command,
            "threadId": THREAD, "messageId": format!("{command}:message"),
            "text": "hello", "attachments": [], "createdBy": "user",
            "creationSource": "web", "dispatchMode": {"type": "start_immediately"}})
    );
    assert_eq!(
        payload(&prompt)["dispatchMode"],
        json!({"type": "queue_after_active"})
    );
}

#[test]
fn the_same_chat_command_reaches_t3_with_the_same_ids() {
    let prompt = Command::Prompt {
        text: "again".to_owned(),
        attachments: Vec::new(),
        origin: None,
    };
    let first = dispatch(&idle(), &[], id(9), &prompt).unwrap();
    assert_eq!(first, dispatch(&idle(), &[], id(9), &prompt).unwrap());
    assert_ne!(
        first.command_id,
        dispatch(&idle(), &[], id(10), &prompt).unwrap().command_id
    );
}

#[test]
fn a_prompt_names_its_files_and_is_bounded() {
    let prompt = Command::Prompt {
        text: "look".to_owned(),
        attachments: vec![Attachment {
            path: "/tmp/a.png".to_owned(),
        }],
        origin: None,
    };
    assert_eq!(payload(&prompt)["text"], "look\n\n/tmp/a.png");
    let empty = Command::Prompt {
        text: "  ".to_owned(),
        attachments: Vec::new(),
        origin: None,
    };
    assert_eq!(
        dispatch(&idle(), &[], id(1), &empty),
        Err(Rejection::EmptyPrompt)
    );
    let long = Command::Prompt {
        text: "x".repeat(MAX_PROMPT_SIZE + 1),
        attachments: Vec::new(),
        origin: None,
    };
    assert_eq!(
        dispatch(&idle(), &[], id(1), &long),
        Err(Rejection::PromptTooLong)
    );
}

#[test]
fn cancel_interrupts_the_run_under_way() {
    assert_eq!(
        payload(&Command::Cancel),
        json!({"type": "run.interrupt", "commandId": "sheprd:00000000-0000-0000-0000-000000000007",
            "threadId": THREAD, "runId": "run-2", "holdQueue": true})
    );
    assert_eq!(
        dispatch(&idle(), &[], id(1), &Command::Cancel),
        Err(Rejection::NotWorking)
    );
}

#[test]
fn an_approval_is_answered_with_one_of_its_decisions() {
    let thread = asking();
    let permissions = crate::transcript::permissions(&thread);
    assert_eq!(permissions.len(), 2, "{permissions:?}");
    let approval = &permissions[0];
    assert_eq!(approval.title, "Run `ls`?");
    assert_eq!(
        approval
            .options
            .iter()
            .map(|option| (option.id.as_str(), option.kind))
            .collect::<Vec<_>>(),
        [
            ("accept", PermissionKind::AllowOnce),
            ("decline", PermissionKind::RejectOnce)
        ]
    );
    let answer = |option: &str| Command::Answer {
        option: option.to_owned(),
        permission: approval.id,
    };
    let sent = dispatch(&thread, &[], id(2), &answer("accept")).unwrap();
    assert_eq!(
        serde_json::to_value(sent).unwrap(),
        json!({"type": "runtime-request.respond",
            "commandId": "sheprd:00000000-0000-0000-0000-000000000002",
            "threadId": THREAD, "requestId": "request-1", "decision": "accept"})
    );
    assert_eq!(
        dispatch(&thread, &[], id(2), &answer("acceptAlways")),
        Err(Rejection::UnknownOption)
    );
    // An answered request, and an entry that is no request, take nothing.
    for entry in [3, 99] {
        let stale = Command::Answer {
            option: "accept".to_owned(),
            permission: PermissionId(entry),
        };
        assert_eq!(
            dispatch(&thread, &[], id(2), &stale),
            Err(Rejection::UnknownPermission)
        );
    }
}

#[test]
fn an_approval_naming_no_options_takes_every_decision() {
    let thread = thread(
        json!([]),
        json!([{"id": "request-1", "status": "pending"}]),
        json!([{"id": "item-1", "ordinal": 1, "type": "approval_request",
            "requestId": "request-1", "requestKind": "command"}]),
    );
    let permission = &crate::transcript::permissions(&thread)[0];
    let options: Vec<&str> = permission
        .options
        .iter()
        .map(|option| option.id.as_str())
        .collect();
    assert_eq!(options, ["accept", "acceptForSession", "decline", "cancel"]);
    assert_eq!(permission.allow_option().unwrap().id, "accept");
}

#[test]
fn a_question_is_answered_by_question_id_with_the_options_value() {
    let thread = asking();
    let question = &crate::transcript::permissions(&thread)[1];
    let asked = question.question.as_ref().unwrap();
    assert_eq!(asked.text, "Which color?");
    assert_eq!(asked.header.as_deref(), Some("Color"));
    let answer = |answers: &[&str]| Command::AnswerQuestion {
        answers: answers.iter().map(|answer| (*answer).to_owned()).collect(),
        permission: question.id,
    };
    let answers_of = |command: &Command| {
        let sent = dispatch(&thread, &[], id(3), command).unwrap();
        serde_json::to_value(sent).unwrap()["answers"].clone()
    };
    assert_eq!(answers_of(&answer(&["Red"])), json!({"color": "red"}));
    assert_eq!(answers_of(&answer(&["Blue"])), json!({"color": "Blue"}));
    assert_eq!(answers_of(&answer(&["Green"])), json!({"color": "Green"}));
    assert_eq!(
        dispatch(&thread, &[], id(3), &answer(&["Red", "Blue"])),
        Err(Rejection::InvalidAnswer)
    );
    // Several questions at once are answered in T3 Code.
    let pair = Command::AnswerQuestion {
        answers: vec!["x".to_owned()],
        permission: PermissionId(4),
    };
    assert_eq!(
        dispatch(&thread, &[], id(3), &pair),
        Err(Rejection::UnknownPermission)
    );
}

#[test]
fn a_question_taking_several_answers_sends_a_list() {
    let thread = thread(
        json!([]),
        json!([{"id": "request-1", "status": "pending"}]),
        json!([{"id": "item-1", "ordinal": 1, "type": "user_input_request",
            "requestId": "request-1",
            "questions": [{"id": "q", "header": "Q", "question": "Which?", "multiSelect": true,
                "options": [{"label": "A", "description": "a"},
                    {"label": "B", "description": "b", "value": "bee"}]}]}]),
    );
    let command = Command::AnswerQuestion {
        answers: vec!["A".to_owned(), "B".to_owned()],
        permission: PermissionId(1),
    };
    let sent = serde_json::to_value(dispatch(&thread, &[], id(4), &command).unwrap()).unwrap();
    assert_eq!(sent["answers"], json!({"q": ["A", "bee"]}));
}

#[test]
fn the_threads_provider_and_model_are_settings() {
    let options = options(&idle(), &providers());
    let ids: Vec<&str> = options.iter().map(|option| option.id.as_str()).collect();
    assert_eq!(ids, [PROVIDER, MODEL]);
    assert_eq!(options[0].current_name(), "Fake");
    assert_eq!(options[1].current_name(), "Default");
    assert_eq!(options[1].category, OptionCategory::Model);
    let OptionKind::Select { choices, .. } = &options[0].kind else {
        panic!("a choice");
    };
    // Disabled providers, and those with no model, are not offered.
    let offered: Vec<&str> = choices.iter().map(|choice| choice.id.as_str()).collect();
    assert_eq!(offered, ["fakeacp", "other"]);
    // With one provider, only its model is a setting; with none, nothing.
    assert_eq!(options_for(1), [MODEL]);
    assert!(super::options(&idle(), &[]).is_empty());
}

#[test]
fn a_thread_whose_agent_cannot_run_can_still_move_to_another() {
    let thread = thread(json!([]), json!([]), json!([]));
    let healthy: Vec<Provider> = providers()
        .into_iter()
        .filter(|provider| provider.instance_id == "other")
        .collect();
    let options = options(&thread, &healthy);
    let ids: Vec<&str> = options.iter().map(|option| option.id.as_str()).collect();
    assert_eq!(ids, [PROVIDER]);
    // The agent it runs is still named, though no longer offered.
    assert_eq!(options[0].current_name(), "fakeacp");
    let switch = Command::SetOption {
        id: PROVIDER.to_owned(),
        value: OptionValue::Choice {
            id: "other".to_owned(),
        },
    };
    let sent = serde_json::to_value(dispatch(&thread, &healthy, id(1), &switch).unwrap()).unwrap();
    assert_eq!(sent["type"], "provider.switch");
}

fn options_for(count: usize) -> Vec<String> {
    let providers: Vec<Provider> = providers().into_iter().take(count).collect();
    options(&idle(), &providers)
        .into_iter()
        .map(|option| option.id)
        .collect()
}

#[test]
fn a_model_is_set_in_place_and_a_provider_takes_the_thread() {
    let set = |id: &str, choice: &str| Command::SetOption {
        id: id.to_owned(),
        value: OptionValue::Choice {
            id: choice.to_owned(),
        },
    };
    let model = payload(&set(MODEL, "fast"));
    assert_eq!(model["type"], "thread.model-selection.set");
    assert_eq!(
        model["modelSelection"],
        json!({"instanceId": "fakeacp", "model": "fast"})
    );
    let provider = payload(&set(PROVIDER, "other"));
    assert_eq!(provider["type"], "provider.switch");
    assert_eq!(
        provider["modelSelection"],
        json!({"instanceId": "other", "model": "large"})
    );
    for (setting, choice) in [(MODEL, "large"), (PROVIDER, "off"), ("mode", "fast")] {
        assert_eq!(
            dispatch(&running(), &providers(), id(5), &set(setting, choice)),
            Err(Rejection::UnknownSetting),
            "{setting} {choice}"
        );
    }
}

#[test]
fn commands_t3_has_no_counterpart_for_are_refused() {
    let thread = running();
    for (command, rejection) in [
        (
            Command::SetMode {
                id: "plan".to_owned(),
            },
            Rejection::UnknownMode,
        ),
        (Command::Unqueue { id: id(1) }, Rejection::UnknownQueued),
        (
            Command::StopActivity {
                call_id: "x".to_owned(),
            },
            Rejection::UnknownActivity,
        ),
        (Command::SetAutoAccept { on: true }, Rejection::NotReady),
    ] {
        assert_eq!(dispatch(&thread, &[], id(6), &command), Err(rejection));
    }
}

#[test]
fn a_permission_keeps_its_entrys_id() {
    let thread = asking();
    let ids: Vec<PermissionId> = crate::transcript::permissions(&thread)
        .iter()
        .map(|permission| permission.id)
        .collect();
    let entries: Vec<EntryId> = thread.items().iter().map(|(id, _)| *id).collect();
    assert_eq!(
        ids,
        [PermissionId(entries[0].0), PermissionId(entries[1].0)]
    );
}

mod recorded;

#[test]
fn a_guests_words_never_reach_t3_however_they_are_sent() {
    let guest = sheprd_chat_protocol::Guest::new("Bob", "tma");
    let relayed = Command::GuestPrompt {
        text: "hi".to_owned(),
        guest: guest.clone(),
    };
    let named = Command::Prompt {
        text: "hi".to_owned(),
        attachments: Vec::new(),
        origin: Some(Origin::guest(guest)),
    };
    for command in [relayed, named] {
        assert_eq!(
            dispatch(&idle(), &[], id(1), &command),
            Err(Rejection::Unsupported)
        );
    }
}
