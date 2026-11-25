use super::*;
use crate::control::client::exit;
use sheprd_remote_control::control::DeviceId;

fn phone() -> Phone {
    Phone {
        device: DeviceId("d1".into()),
        name: "iPhone".into(),
        active_ms: 1,
    }
}

fn driven(outcome: Outcome) -> Answer {
    Answer::Driven {
        phone: phone(),
        outcome,
    }
}

fn shows(screen: Option<Screen>, locked: bool) -> Outcome {
    Outcome::Done {
        state: PhoneState { screen, locked },
    }
}

#[test]
fn no_phone_is_nothing_yet() {
    for answer in [Answer::NoPhone, Answer::Phones { phones: Vec::new() }] {
        let output = output(&answer, Asked::Phones).unwrap();
        assert_eq!(output.status, exit::NOTHING);
        assert!(output.text.starts_with("No phone"), "{}", output.text);
    }
}

#[test]
fn phones_are_listed_by_name_and_id() {
    let answer = Answer::Phones {
        phones: vec![phone()],
    };
    let output = output(&answer, Asked::Phones).unwrap();
    assert_eq!(output.text, "iPhone\td1");
    assert_eq!(output.json["phones"][0]["device"], "d1");
}

#[test]
fn what_the_phone_shows_is_said_plainly() {
    let chat = Some(Screen::Chat {
        pane: "w1-p2".into(),
    });
    let state = output(&driven(shows(chat.clone(), false)), Asked::State).unwrap();
    assert_eq!(state.text, "iPhone shows chat w1-p2");
    assert_eq!(state.status, 0);
    let elsewhere = output(&driven(shows(None, false)), Asked::State).unwrap();
    assert!(
        elsewhere.text.contains("another computer"),
        "{}",
        elsewhere.text
    );
    let locked = output(&driven(shows(None, true)), Asked::State).unwrap();
    assert_eq!(locked.text, "iPhone is locked");
    let opened = output(&driven(shows(chat, false)), Asked::Open).unwrap();
    assert_eq!(opened.text, "iPhone shows chat w1-p2");
    let shown = output(&driven(shows(None, false)), Asked::Show).unwrap();
    assert_eq!(shown.text, "Shown on iPhone");
}

#[test]
fn a_refusal_fails_with_its_reason() {
    let refused = |reason| output(&driven(Outcome::Refused { reason }), Asked::Open).unwrap();
    let locked = refused(Refusal::Locked);
    assert_eq!(locked.status, exit::REFUSED);
    assert!(locked.text.contains("locked"), "{}", locked.text);
    assert_eq!(locked.json["outcome"]["reason"], "locked");
    assert!(refused(Refusal::Off).text.contains("no longer lets agents"));
}

#[test]
fn a_named_target_needs_no_pane() {
    let context = Context {
        json: false,
        window: None,
    };
    assert_eq!(
        open(PhoneTarget::Screen(Screen::Settings), &context).unwrap(),
        Command::Open {
            to: Screen::Settings
        }
    );
    assert_eq!(
        open(PhoneTarget::Pane(Some("w1-p2".into())), &context).unwrap(),
        Command::OpenPane {
            pane: "w1-p2".into()
        }
    );
    assert_eq!(
        open(PhoneTarget::Changes(Some("w1".into())), &context).unwrap(),
        Command::Open {
            to: Screen::Changes {
                workspace: "w1".into()
            }
        }
    );
}
