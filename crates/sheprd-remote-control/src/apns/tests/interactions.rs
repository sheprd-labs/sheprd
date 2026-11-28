use super::*;
use crate::{app::config::ActivityDetail, push::Privacy};
use sheprd_chat_protocol::{
    Change, Entry, EntryId, EntryKind, Transcript, Turn, TurnEnd, TurnSummary,
};

fn running_transcript() -> Transcript {
    Transcript {
        turn: Some(Turn {
            started_ms: 1_800_000_000_000,
        }),
        ..Default::default()
    }
}

#[test]
fn urgent_blocker_populates_live_activity_and_clears_on_newer_entry_or_turn_end() {
    let mut transcript = running_transcript();
    let urgent_entry = Entry {
        id: EntryId(2),
        kind: EntryKind::Agent {
            text: ":::urgent Tap your YubiKey\npreset: yubikey\nWaiting on push\n:::".to_owned(),
        },
    };
    let live = decide(
        &CHAT,
        "studio",
        "AA:BB",
        &transcript,
        &Change::Push {
            entry: urgent_entry.clone(),
        },
        1_800_000_005_000,
    )
    .expect("urgent blocker emits Live Activity update");
    assert!(live.urgent());
    assert_eq!(
        live.state.detail.blocker_title.as_deref(),
        Some("Tap your YubiKey")
    );
    assert_eq!(live.state.detail.blocker_kind.as_deref(), Some("yubikey"));

    // Wire JSON matches Swift `ChatActivityState` camelCase keys.
    let json = serde_json::to_value(&live.state).unwrap();
    assert_eq!(json["blockerTitle"], "Tap your YubiKey");
    assert_eq!(json["blockerKind"], "yubikey");

    // Generic privacy hides the chat-quoted blocker title.
    let redacted = live
        .clone()
        .redacted(Privacy::Generic, ActivityDetail::Detailed);
    assert_eq!(redacted.state.detail.blocker_title, None);

    transcript.entries.push(urgent_entry);

    // A newer agent entry clears the auto_clear blocker.
    let cleared = decide(
        &CHAT,
        "studio",
        "AA:BB",
        &transcript,
        &Change::Push {
            entry: Entry {
                id: EntryId(3),
                kind: EntryKind::Agent {
                    text: "Push succeeded.".to_owned(),
                },
            },
        },
        1_800_000_010_000,
    )
    .expect("clearing blocker emits update");
    assert_eq!(cleared.state.detail.blocker_title, None);
    assert_eq!(cleared.state.detail.blocker_kind, None);
    assert!(!cleared.urgent());

    // Turn end also guarantees blocker fields are cleared.
    let ended = decide(
        &CHAT,
        "studio",
        "AA:BB",
        &transcript,
        &Change::Push {
            entry: Entry {
                id: EntryId(4),
                kind: EntryKind::Turn(TurnSummary {
                    end: TurnEnd::Done,
                    duration_ms: 10_000,
                    ended_ms: Some(1_800_000_012_000),
                    tokens: None,
                }),
            },
        },
        1_800_000_012_000,
    )
    .expect("turn end emits end event");
    assert_eq!(ended.state.detail.blocker_title, None);
    assert_eq!(ended.state.detail.blocker_kind, None);
}

#[test]
fn progress_directive_populates_and_clears_progress_percent() {
    let mut transcript = running_transcript();
    let progress_entry = Entry {
        id: EntryId(2),
        kind: EntryKind::Agent {
            text: ":::progress Running tests\npercent: 68%\nstep: chat\n:::".to_owned(),
        },
    };
    let live = decide(
        &CHAT,
        "studio",
        "AA:BB",
        &transcript,
        &Change::Push {
            entry: progress_entry.clone(),
        },
        1_800_000_005_000,
    )
    .expect("progress emits update");
    assert_eq!(live.state.detail.progress_percent, Some(68));
    let json = serde_json::to_value(&live.state).unwrap();
    assert_eq!(json["progressPercent"], 68);

    transcript.entries.push(progress_entry);

    let done_live = decide(
        &CHAT,
        "studio",
        "AA:BB",
        &transcript,
        &Change::Push {
            entry: Entry {
                id: EntryId(3),
                kind: EntryKind::Agent {
                    text: ":::progress All tests passed\ndone: true\n:::".to_owned(),
                },
            },
        },
        1_800_000_010_000,
    )
    .expect("done clears progressPercent");
    assert_eq!(done_live.state.detail.progress_percent, None);
}
