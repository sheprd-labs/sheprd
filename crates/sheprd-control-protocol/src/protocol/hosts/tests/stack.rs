use super::*;

#[test]
fn merge_and_queue_requests_have_a_stable_wire_form() {
    for (op, wire) in [
        (HostOp::PrMerge, r#"{"op":"pr_merge"}"#),
        (
            HostOp::PrQueue {
                change: QueueOp::Add,
            },
            r#"{"op":"pr_queue","change":"add"}"#,
        ),
        (
            HostOp::PrQueue {
                change: QueueOp::Remove,
            },
            r#"{"op":"pr_queue","change":"remove"}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&op).unwrap(), wire);
        assert_eq!(serde_json::from_str::<HostOp>(wire).unwrap(), op);
    }
    assert!(serde_json::from_str::<HostOp>(r#"{"op":"pr_queue","change":"jump"}"#).is_err());
}

#[test]
fn an_older_apps_pull_request_without_stack_or_queue_still_reads() {
    let pr: PrInfo = serde_json::from_value(serde_json::json!({
        "number": 8, "url": "https://github.com/example/project/pull/8", "title": "Title",
        "state": "Open", "draft": false, "head": "feature", "base": "main",
        "additions": 1, "deletions": 2, "changed_files": 3,
        "review": null, "merge_status": null, "checks": []
    }))
    .unwrap();
    assert_eq!((pr.stack, pr.queue), (None, None));
}

#[test]
fn a_stacked_pull_request_round_trips_its_layers_and_queue_entry() {
    let stack = StackInfo {
        number: 950,
        base: "main".into(),
        size: 2,
        position: 2,
        verdict: "blocked".into(),
        ready: None,
        blocked_by: Some(1362),
        reason: Some("checks_failed".into()),
        layers: vec![LayerInfo {
            position: 1,
            number: 1362,
            title: "Client".into(),
            status: "checks_failed".into(),
        }],
    };
    let queue = QueueInfo {
        position: 2,
        state: "awaiting_checks".into(),
        eta_seconds: Some(600),
    };
    let wire = serde_json::to_value((&stack, &queue)).unwrap();
    assert_eq!(wire[0]["blocked_by"], 1362);
    assert_eq!(wire[1]["eta_seconds"], 600);
    let (read_stack, read_queue): (StackInfo, QueueInfo) = serde_json::from_value(wire).unwrap();
    assert_eq!((read_stack, read_queue), (stack, queue));
}
