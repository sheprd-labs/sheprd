use super::*;

fn started(dir: &std::path::Path, load: bool) -> Harness {
    let mut h = harness_with(Config {
        load: load.then(|| "s1".to_owned()),
        require_load: load,
        ..config()
    });
    h.session.keep_history(dir.to_owned());
    h.session.start();
    let id = h.last_request("initialize")["id"].as_i64().unwrap();
    h.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    h
}

#[test]
fn only_a_loaded_session_gets_its_saved_conversation_and_never_double_replays_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = started(dir.path(), false);
    let id = h.last_request("session/new")["id"].as_i64().unwrap();
    h.respond(id, json!({"sessionId":"s1"}));
    let _sub = h.subscribe(7, false, None);
    let turn = prompt(&mut h, 7, "before reboot");
    h.agent_says("saved answer");
    h.respond(turn, json!({"stopReason":"end_turn"}));
    drop(h);

    // A later build must still open the original v1 checkpoint, whose
    // provider, cwd and session identity appeared only in its filename.
    let path = std::fs::read_dir(dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut original: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    original.as_object_mut().unwrap().remove("identity");
    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();

    for replay in [false, true] {
        let mut h = started(dir.path(), true);
        assert!(h.transcript().entries.is_empty());
        let load = h.last_request("session/load");
        assert_eq!(load["params"]["sessionId"], "s1");
        if replay {
            h.update(json!({"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"before reboot"}}));
            h.agent_says("saved answer");
        }
        h.respond(load["id"].as_i64().unwrap(), json!({}));
        for text in ["before reboot", "saved answer"] {
            assert_eq!(
                h.transcript()
                    .entries
                    .iter()
                    .filter(|entry| entry.kind.text() == Some(text))
                    .count(),
                1
            );
        }
        assert!(h.transcript().permissions.is_empty());
        assert!(h.transcript().queued.is_empty());
    }
}
