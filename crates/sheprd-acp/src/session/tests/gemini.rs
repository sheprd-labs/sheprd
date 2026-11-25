use sheprd_chat_protocol::{OptionCategory, OptionValue};

use super::*;

fn gemini_config() -> Config {
    Config {
        agent: "Gemini".to_owned(),
        gemini_auth_method: Some("oauth-personal".to_owned()),
        harness: Some(Agent::Gemini),
        login_hint: Some("run `gemini` once to sign in".to_owned()),
        ..config()
    }
}

fn gemini_init_result() -> Value {
    json!({
        "protocolVersion": 1,
        "agentCapabilities": {"loadSession": true},
        "authMethods": [
            {"id": "oauth-personal", "name": "Log in with Google"},
            {"id": "gemini-api-key", "name": "Gemini API key"},
            {"id": "vertex-ai", "name": "Vertex AI"}
        ]
    })
}

#[test]
fn gemini_authenticates_when_new_session_requires_sign_in() {
    let mut harness = harness_with(gemini_config());
    harness.session.start();
    let init_id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(init_id, gemini_init_result());

    // First session/new fails with auth_required; session runs ACP authenticate.
    let first_new = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.fail(
        first_new,
        wire::Error::auth_required().data(json!("missing key")),
    );
    let auth = harness.last_request("authenticate");
    assert_eq!(auth["params"]["methodId"], "oauth-personal");
    // Gemini CLI's authenticate handler returns null on success.
    harness.respond(auth["id"].as_i64().unwrap(), Value::Null);

    let retry_new = harness.last_request("session/new")["id"].as_i64().unwrap();
    assert_ne!(retry_new, first_new);
    harness.respond(
        retry_new,
        json!({
            "sessionId": "gemini-1",
            "modes": {
                "currentModeId": "default",
                "availableModes": [{"id": "default", "name": "Default"}]
            },
            "models": {
                "currentModelId": "gemini-2.5-pro",
                "availableModels": [
                    {"modelId": "gemini-2.5-pro", "name": "Gemini 2.5 Pro"},
                    {"modelId": "gemini-2.5-flash", "name": "Gemini 2.5 Flash"}
                ]
            }
        }),
    );

    assert_eq!(harness.transcript().status, Status::Idle);
    assert_eq!(
        harness.transcript().session.as_ref().unwrap().id,
        "gemini-1"
    );
    assert_eq!(
        harness
            .transcript()
            .option(OptionCategory::Model)
            .unwrap()
            .current_name(),
        "Gemini 2.5 Pro"
    );

    // Changing the model on a legacy `models` server uses `session/set_model`.
    harness.command(
        0,
        Command::SetOption {
            id: "model".into(),
            value: OptionValue::Choice {
                id: "gemini-2.5-flash".into(),
            },
        },
    );
    let set_model = harness.last_request("session/set_model");
    assert_eq!(set_model["params"]["sessionId"], "gemini-1");
    assert_eq!(set_model["params"]["modelId"], "gemini-2.5-flash");
    harness.respond(set_model["id"].as_i64().unwrap(), json!({}));
    assert_eq!(
        harness
            .transcript()
            .option(OptionCategory::Model)
            .unwrap()
            .current_name(),
        "Gemini 2.5 Flash"
    );
}

#[test]
fn gemini_auth_failure_reports_hint_and_reason_without_looping() {
    let mut harness = harness_with(gemini_config());
    harness.session.start();
    let init_id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(init_id, gemini_init_result());

    let first_new = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.fail(
        first_new,
        wire::Error::new(
            wire::Error::auth_required().code.into(),
            "Gemini API key is missing or not configured.",
        ),
    );
    let auth = harness.last_request("authenticate");
    harness.fail(
        auth["id"].as_i64().unwrap(),
        wire::Error::new(
            wire::Error::auth_required().code.into(),
            "Browser sign-in cancelled",
        ),
    );

    let Status::Failed { message } = &harness.transcript().status else {
        panic!("expected failure");
    };
    assert!(
        message.contains("run `gemini` once to sign in"),
        "{message}"
    );
    assert!(message.contains("Browser sign-in cancelled"), "{message}");
    assert!(harness.sent().is_empty());
}
