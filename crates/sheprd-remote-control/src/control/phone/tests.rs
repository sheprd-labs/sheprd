use super::*;
use serde_json::json;

#[test]
fn screens_are_tagged_by_name_with_their_ids() {
    let chat = Screen::Chat {
        pane: "w1-p2".into(),
    };
    assert_eq!(
        serde_json::to_value(&chat).unwrap(),
        json!({"screen": "chat", "pane": "w1-p2"})
    );
    let tab: Screen = serde_json::from_value(json!({"screen": "pull_requests"})).unwrap();
    assert_eq!(tab, Screen::PullRequests);
    let changes: Screen =
        serde_json::from_value(json!({"screen": "changes", "workspace": "w1"})).unwrap();
    assert_eq!(
        changes,
        Screen::Changes {
            workspace: "w1".into()
        }
    );
}

#[test]
fn ids_and_notes_out_of_bounds_are_not_valid() {
    assert!(Screen::Home.is_valid());
    assert!(
        !Screen::Chat {
            pane: String::new()
        }
        .is_valid()
    );
    assert!(
        !Screen::Term {
            pane: "a\nb".into()
        }
        .is_valid()
    );
    assert!(
        !Screen::Workspace {
            workspace: "w".repeat(MAX_ID_BYTES + 1)
        }
        .is_valid()
    );
    assert!(valid_text("Look at the diff"));
    assert!(!valid_text("   "));
    assert!(valid_text(&"é".repeat(MAX_TEXT_CHARS)));
    assert!(!valid_text(&"é".repeat(MAX_TEXT_CHARS + 1)));
}

#[test]
fn an_ask_names_its_phone_only_when_given() {
    let ask = Ask::Drive {
        device: None,
        command: Command::Open {
            to: Screen::Settings,
        },
    };
    assert_eq!(
        serde_json::to_value(&ask).unwrap(),
        json!({"ask": "drive", "command": {"command": "open", "to": {"screen": "settings"}}})
    );
    let back: Ask = serde_json::from_value(json!({
        "ask": "drive", "device": "iPad", "command": {"command": "open_pane", "pane": "w1-p1"}
    }))
    .unwrap();
    assert_eq!(
        back,
        Ask::Drive {
            device: Some("iPad".into()),
            command: Command::OpenPane {
                pane: "w1-p1".into()
            },
        }
    );
}

#[test]
fn a_phones_outcome_reads_its_state_and_unknown_refusals() {
    let done: Outcome = serde_json::from_value(json!({
        "outcome": "done", "state": {"screen": {"screen": "term", "pane": "p"}}
    }))
    .unwrap();
    assert_eq!(
        done,
        Outcome::Done {
            state: PhoneState {
                screen: Some(Screen::Term { pane: "p".into() }),
                locked: false,
            },
        }
    );
    let refused: Outcome =
        serde_json::from_value(json!({"outcome": "refused", "reason": "busy"})).unwrap();
    assert_eq!(
        refused,
        Outcome::Refused {
            reason: Refusal::Other
        }
    );
    assert_eq!(
        serde_json::to_value(PhoneState {
            screen: None,
            locked: true
        })
        .unwrap(),
        json!({"locked": true})
    );
}

#[test]
fn settings_and_guidance_have_stable_wire_targets() {
    for page in SettingsPage::ALL {
        assert_eq!(SettingsPage::parse(page.name()), Some(*page));
        let screen = Screen::SettingsPage { page: *page };
        assert_eq!(
            serde_json::to_value(&screen).unwrap(),
            json!({"screen":"settings_page", "page":page.name()})
        );
        assert_eq!(
            serde_json::from_value::<Screen>(serde_json::to_value(&screen).unwrap()).unwrap(),
            screen
        );
        assert!(screen.is_valid());
    }
    for target in Highlight::ALL {
        assert_eq!(Highlight::parse(target.name()), Some(*target));
        assert!(SettingsPage::ALL.contains(&target.page()));
        let drive = Drive::Guide {
            text: "Look here".into(),
            highlight: Some(*target),
            from: None,
        };
        assert_eq!(
            serde_json::to_value(&drive).unwrap(),
            json!({"command":"guide", "text":"Look here", "highlight":target.name()})
        );
        assert_eq!(
            serde_json::from_value::<Drive>(serde_json::to_value(&drive).unwrap()).unwrap(),
            drive
        );
    }
    assert!(
        serde_json::from_value::<Screen>(json!({"screen":"settings_page","page":"unknown"}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<Drive>(json!({"command":"highlight","target":"unknown"})).is_err()
    );
    assert_eq!(
        serde_json::to_value(Drive::ClearGuide).unwrap(),
        json!({"command":"clear_guide"})
    );
}
