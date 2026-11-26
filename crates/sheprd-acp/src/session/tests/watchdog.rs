use std::time::Duration;

use super::*;
use crate::session::watchdog::{
    DEFAULT_CANCEL_AFTER, WARN_AFTER, cancel_after_from, describe_duration,
};

fn ready_antigravity() -> Harness {
    let mut harness = harness_with(Config {
        agent: "Antigravity".to_owned(),
        harness: Some(Agent::Antigravity),
        ..config()
    });
    harness
        .session
        .watchdog
        .set_cancel_after(Some(DEFAULT_CANCEL_AFTER));
    harness.session.start();
    let init = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(init, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let new = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(new, json!({"sessionId": "s1"}));
    harness
}

#[test]
fn cancel_after_env_parses_default_zero_custom_and_invalid() {
    assert_eq!(cancel_after_from(None), Some(DEFAULT_CANCEL_AFTER));
    assert_eq!(cancel_after_from(Some("")), Some(DEFAULT_CANCEL_AFTER));
    assert_eq!(cancel_after_from(Some("   ")), Some(DEFAULT_CANCEL_AFTER));
    assert_eq!(cancel_after_from(Some("0")), None);
    assert_eq!(cancel_after_from(Some(" 0 ")), None);
    assert_eq!(
        cancel_after_from(Some("900")),
        Some(Duration::from_secs(900))
    );
    assert_eq!(cancel_after_from(Some("bogus")), Some(DEFAULT_CANCEL_AFTER));
    assert_eq!(cancel_after_from(Some("-5")), Some(DEFAULT_CANCEL_AFTER));
}

#[test]
fn describe_duration_formats_seconds_minutes_and_hours() {
    assert_eq!(describe_duration(Duration::from_secs(1)), "1 second");
    assert_eq!(describe_duration(Duration::from_secs(45)), "45 seconds");
    assert_eq!(describe_duration(Duration::from_secs(60)), "1 minute");
    assert_eq!(describe_duration(WARN_AFTER), "10 minutes");
    assert_eq!(describe_duration(DEFAULT_CANCEL_AFTER), "1 hour");
    assert_eq!(describe_duration(Duration::from_secs(2 * 3600)), "2 hours");
    assert_eq!(
        describe_duration(Duration::from_secs(90 * 60)),
        "90 minutes"
    );
}

#[test]
fn quiet_antigravity_turn_warns_then_cancels_without_dropping_queued_prompts() {
    let mut harness = ready_antigravity();
    let turn = prompt(&mut harness, 1, "ship the fix");

    // Before any assistant reply arrives, the watchdog is armed only after a reply.
    assert_eq!(harness.session.next_tick(), None);

    harness.agent_says("All done!");
    let warn_at = harness.session.next_tick().expect("warn deadline");

    // A fresh chunk resets the deadline before it fires.
    harness.session.tick(warn_at - Duration::from_secs(1));
    assert!(harness.notices().is_empty());
    harness.agent_says(" Really done.");
    let warn_at = harness.session.next_tick().expect("reset warn deadline");

    // Queue a follow-up while the turn stays stuck on a background task.
    harness.command(
        1,
        Command::Prompt {
            origin: None,
            text: "follow up".to_owned(),
            attachments: Vec::new(),
        },
    );
    assert_eq!(harness.transcript().queued.len(), 1);

    // 10 minutes of silence posts one informational hint and schedules auto-cancel.
    harness.session.tick(warn_at);
    let notices = harness.notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].0, NoticeLevel::Info);
    assert!(notices[0].1.contains("10 minutes"), "{}", notices[0].1);
    assert!(harness.sent().is_empty());

    // Our own warning Notice must not stop the 1-hour auto-cancel deadline.
    let cancel_at = harness.session.next_tick().expect("cancel deadline");
    assert!(cancel_at > warn_at);

    // 1 hour of total silence posts a warning and sends session/cancel while
    // preserving the queued follow-up prompt.
    harness.session.tick(cancel_at);
    let notices = harness.notices();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[1].0, NoticeLevel::Warning);
    assert!(notices[1].1.contains("1 hour"), "{}", notices[1].1);

    let sent = harness.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "session/cancel");
    assert_eq!(sent[0]["params"]["sessionId"], "s1");
    assert_eq!(harness.transcript().queued.len(), 1);
    assert_eq!(harness.session.next_tick(), None);

    // Once agy acp finishes cancelling the stuck turn, the queued prompt runs.
    harness.respond(turn, json!({"stopReason": "cancelled"}));
    assert!(harness.transcript().queued.is_empty());
    let next = harness.last_request("session/prompt");
    assert_eq!(
        next["params"]["prompt"],
        json!([{"type": "text", "text": "follow up"}])
    );
}

#[test]
fn disabling_auto_cancel_still_warns_once_and_never_cancels() {
    let mut harness = ready_antigravity();
    harness.session.watchdog.set_cancel_after(None);
    let _turn = prompt(&mut harness, 1, "run a long background server");
    harness.agent_says("Server started.");

    let warn_at = harness.session.next_tick().expect("warn deadline");
    harness.session.tick(warn_at);
    assert_eq!(harness.notices().len(), 1);
    assert_eq!(harness.notices()[0].0, NoticeLevel::Info);
    assert_eq!(harness.session.next_tick(), None);

    harness
        .session
        .tick(warn_at + Duration::from_secs(10 * 3600));
    assert_eq!(harness.notices().len(), 1);
    assert!(harness.sent().is_empty());
}

#[test]
fn watchdog_skips_non_antigravity_running_tools_and_pending_permissions() {
    // Non-Antigravity agents (e.g. Claude) never schedule the watchdog.
    let mut claude = ready();
    let _turn = prompt(&mut claude, 1, "hello");
    claude.agent_says("hi");
    assert_eq!(claude.session.next_tick(), None);

    // In-progress foreground tool blocks the watchdog until a reply follows it.
    let mut agy = ready_antigravity();
    let _turn = prompt(&mut agy, 1, "check repo");
    agy.agent_says("Checking...");
    assert!(agy.session.next_tick().is_some());
    agy.update(json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "call-1",
        "title": "Run cargo test",
        "kind": "execute",
        "status": "in_progress"
    }));
    assert_eq!(agy.session.next_tick(), None);

    // Completing the tool without a new assistant reply still waits for the model.
    agy.update(json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "call-1",
        "status": "completed"
    }));
    assert_eq!(agy.session.next_tick(), None);

    // Once the assistant streams its reply, a pending permission still blocks it.
    agy.agent_says("Ready to push.");
    assert!(agy.session.next_tick().is_some());
    let _perm = ask_permission(&mut agy, 99);
    assert_eq!(agy.session.next_tick(), None);
}

#[test]
fn background_command_turn_warns_cancels_and_runs_queued_prompt() {
    // Matches `agy acp`'s real ACP update sequence for a backgrounded
    // `run_command`: the tool call starts in_progress, completes once the
    // command moves to the background, the assistant replies, and `agy acp`
    // holds `session/prompt` open while waiting for the background task.
    let mut harness = ready_antigravity();
    let turn = prompt(&mut harness, 1, "start the background job");

    harness.update(json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "cmd-bg-1",
        "title": "Run background build",
        "kind": "execute",
        "status": "in_progress"
    }));
    assert_eq!(harness.session.next_tick(), None);

    harness.update(json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "cmd-bg-1",
        "status": "completed",
        "content": [{
            "type": "content",
            "content": {
                "type": "text",
                "text": "Tool is running as a background task with task id: conv/task-1"
            }
        }]
    }));
    // Tool finished backgrounding, but the model has not replied yet.
    assert_eq!(harness.session.next_tick(), None);

    harness.agent_says("Started the background build.");
    let warn_at = harness.session.next_tick().expect("warn deadline");

    // Queue a follow-up prompt while `session/prompt` remains held open.
    harness.command(
        1,
        Command::Prompt {
            origin: None,
            text: "check build status".to_owned(),
            attachments: Vec::new(),
        },
    );
    assert_eq!(harness.transcript().queued.len(), 1);

    harness.session.tick(warn_at);
    let notices = harness.notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].0, NoticeLevel::Info);
    assert!(notices[0].1.contains("10 minutes"), "{}", notices[0].1);
    assert!(harness.sent().is_empty());

    let cancel_at = harness.session.next_tick().expect("cancel deadline");
    harness.session.tick(cancel_at);
    let notices = harness.notices();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[1].0, NoticeLevel::Warning);
    assert!(notices[1].1.contains("1 hour"), "{}", notices[1].1);

    let sent = harness.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "session/cancel");
    assert_eq!(sent[0]["params"]["sessionId"], "s1");
    assert_eq!(harness.transcript().queued.len(), 1);
    assert_eq!(harness.session.next_tick(), None);

    harness.respond(turn, json!({"stopReason": "cancelled"}));
    assert!(harness.transcript().queued.is_empty());
    let next = harness.last_request("session/prompt");
    assert_eq!(
        next["params"]["prompt"],
        json!([{"type": "text", "text": "check build status"}])
    );
}
