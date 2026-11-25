use super::*;

fn muse() -> Harness {
    harness_with(Config {
        agent: Agent::Muse.display_name().to_owned(),
        harness: Some(Agent::Muse),
        login_hint: Some(Agent::Muse.login_hint().to_owned()),
        ..config()
    })
}

#[test]
fn a_signed_out_muse_session_points_to_the_cli_login() {
    let mut harness = muse();
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({
            "protocolVersion": 1, "agentCapabilities": {"loadSession": true},
            "authMethods": [{"id": "muse-login", "name": "Log in to Muse"}]
        }),
    );
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.fail(
        id,
        wire::Error::new(-32000, "log in to Muse before opening a session"),
    );
    let Status::Failed { message } = &harness.transcript().status else {
        panic!("expected login guidance");
    };
    assert!(message.contains("muse login"), "{message}");
}

#[test]
fn muse_exiting_with_a_closed_acp_gate_explains_the_build_check() {
    let mut harness = muse();
    harness.input(Input::AgentExited {
        code: Some(5),
        harness: HarnessId(0),
        stderr: vec!["muse acp is not available in this build".into()],
    });
    let Status::Exited { message } = &harness.transcript().status else {
        panic!("expected exit guidance");
    };
    assert!(message.contains("muse acp --help"), "{message}");
}
