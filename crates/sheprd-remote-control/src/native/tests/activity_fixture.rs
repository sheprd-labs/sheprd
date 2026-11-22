//! The sub-agents, shells, and agent choices a chat sends a phone, written
//! as the fixture its Swift tests decode, so the phone's chat panel reads
//! what the daemon really sends.

use super::fixtures::{check, cursor, dir};
use super::*;
use sheprd_chat_protocol::{
    Activity, ActivityKind, ActivityState, Agent, Change, Command, Harnesses,
};
use wire::Step;

const NAME: &str = "chat_activity";

fn message() -> ServerMessage {
    let running = Activity {
        call_id: "toolu_1".into(),
        detail: Some("Reading crates/sheprd-acp/src/main.rs".into()),
        finished_ms: None,
        kind: ActivityKind::Subagent,
        label: "Find where the rail reads activities".into(),
        started_ms: 1_770_000_000_000,
        state: ActivityState::Running,
        stoppable: true,
    };
    let ended = Activity {
        call_id: "toolu_2".into(),
        detail: None,
        finished_ms: Some(1_770_000_060_000),
        kind: ActivityKind::Shell,
        label: "cargo test -p sheprd-chat-protocol".into(),
        started_ms: 1_770_000_010_000,
        state: ActivityState::Failed,
        stoppable: false,
    };
    ServerMessage::ChatChanges {
        pane: "p1".into(),
        changes: vec![
            Step {
                cursor: cursor(45),
                change: Change::Activity { activity: running },
            },
            Step {
                cursor: cursor(46),
                change: Change::Activity { activity: ended },
            },
            Step {
                cursor: cursor(47),
                change: Change::Harnesses {
                    harnesses: Some(Harnesses {
                        available: vec![Agent::Claude, Agent::Codex],
                        current: Some(Agent::Claude),
                    }),
                },
            },
        ],
    }
}

#[test]
fn activity_changes_match_the_swift_fixture() {
    check(NAME, &message());
}

#[test]
fn the_activity_fixture_reads_back_as_what_wrote_it() {
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        return;
    }
    let text = std::fs::read_to_string(dir().join(format!("{NAME}.json"))).unwrap();
    let message: ServerMessage = serde_json::from_str(&text).unwrap();
    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&message).unwrap()),
        text
    );
}

#[test]
fn a_phone_stop_reads_as_the_chat_command() {
    let command: Command =
        serde_json::from_str(r#"{"command":"stop_activity","call_id":"toolu_1"}"#).unwrap();
    assert_eq!(
        command,
        Command::StopActivity {
            call_id: "toolu_1".into()
        }
    );
}
