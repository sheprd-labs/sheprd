use super::*;
use crate::{
    control::phone::{PhoneState, Refusal},
    native::{
        tests::{layout, offline},
        wire::ClientMessage,
    },
};
use serde_json::json;
use sheprd_chat_protocol::EndpointId;
use std::time::Instant;

fn state(dir: &std::path::Path) -> crate::web::Shared {
    offline(dir, layout(EndpointId::random()))
}

fn phone(device: &str, name: &str, active_ms: u64) -> Phone {
    Phone {
        device: DeviceId(device.into()),
        name: name.into(),
        active_ms,
    }
}

fn done() -> Outcome {
    Outcome::Done {
        state: PhoneState::default(),
    }
}

#[test]
fn the_live_socket_carries_drives_and_their_answers_as_json() {
    let drive = ServerMessage::Drive {
        id: 7,
        drive: Drive::Open {
            to: Screen::Chat { pane: "p1".into() },
        },
    };
    assert_eq!(
        serde_json::to_value(&drive).unwrap(),
        json!({"type": "drive", "id": 7, "command": "open", "to": {"screen": "chat", "pane": "p1"}})
    );
    let allowed: ClientMessage =
        serde_json::from_value(json!({"type": "agent_control", "allowed": true})).unwrap();
    assert_eq!(allowed, ClientMessage::AgentControl { allowed: true });
    let driven: ClientMessage = serde_json::from_value(json!({
        "type": "driven", "id": 7, "outcome": "refused", "reason": "locked"
    }))
    .unwrap();
    assert_eq!(
        driven,
        ClientMessage::Driven {
            id: 7,
            outcome: Outcome::Refused {
                reason: Refusal::Locked
            },
        }
    );
}

#[test]
fn a_pane_opens_as_the_chat_or_terminal_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let open = |pane: &str| resolve(&state, Command::OpenPane { pane: pane.into() });
    assert_eq!(
        open("p1"),
        Ok(Drive::Open {
            to: Screen::Chat { pane: "p1".into() }
        })
    );
    assert_eq!(
        open("p2"),
        Ok(Drive::Open {
            to: Screen::Term { pane: "p2".into() }
        })
    );
    assert_eq!(open("p9"), Err(Answer::NotFound));
    assert_eq!(open(""), Err(Answer::Invalid));
}

#[test]
fn a_screen_of_something_not_here_is_never_sent() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let open = |to| resolve(&state, Command::Open { to });
    assert!(
        open(Screen::Changes {
            workspace: "w1".into()
        })
        .is_ok()
    );
    assert!(open(Screen::Settings).is_ok());
    assert_eq!(
        open(Screen::Workspace {
            workspace: "w9".into()
        }),
        Err(Answer::NotFound)
    );
    assert_eq!(
        open(Screen::Term { pane: "p1".into() }),
        Err(Answer::NotFound)
    );
    let show = |text: &str| resolve(&state, Command::Show { text: text.into() });
    assert_eq!(show(" "), Err(Answer::Invalid));
    assert_eq!(
        show("Tests pass"),
        Ok(Drive::Show {
            text: "Tests pass".into()
        })
    );
}

#[test]
fn the_phone_used_last_is_picked_unless_one_is_named() {
    let phones = vec![
        (2, phone("d2", "iPad", 20)),
        (1, phone("d1", "iPhone", 10)),
        (3, phone("d3", "iPhone", 5)),
    ];
    assert_eq!(pick(phones.clone(), None).unwrap().0, 2);
    assert_eq!(pick(phones.clone(), Some("d3")).unwrap().0, 3);
    assert_eq!(pick(phones.clone(), Some("iPad")).unwrap().0, 2);
    assert_eq!(pick(phones.clone(), Some("iPhone")), Err(Answer::Ambiguous));
    assert_eq!(pick(phones, Some("Watch")), Err(Answer::NoPhone));
    assert_eq!(pick(Vec::new(), None), Err(Answer::NoPhone));
}

#[test]
fn each_phone_is_listed_once_by_its_latest_socket() {
    let drivers = Drivers::default();
    let (out, _kept) = mpsc::channel(4);
    for (socket, device, active) in [(1, "d1", 10), (2, "d1", 30), (3, "d2", 20), (4, "d3", 5)] {
        drivers.allow(socket, Some((DeviceId(device.into()), out.clone())), active);
    }
    // A device no longer paired is not listed.
    let name = |device: &DeviceId| (device.0 != "d3").then(|| format!("{} phone", device.0));
    let phones = drivers.phones(name);
    assert_eq!(
        phones,
        vec![
            (2, phone("d1", "d1 phone", 30)),
            (3, phone("d2", "d2 phone", 20))
        ]
    );
    drivers.allow(2, None, 40);
    assert_eq!(drivers.phones(name)[0], (3, phone("d2", "d2 phone", 20)));
}

#[test]
fn a_drive_is_sent_to_its_phone_and_answered_by_it_alone() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 1);
    let (out, mut to_phone) = mpsc::channel(4);
    state
        .drivers
        .allow(5, Some((device.clone(), out.clone())), 1);
    state.drivers.allow(6, Some((device, out)), 0);
    let asking = {
        let state = state.clone();
        std::thread::spawn(move || {
            let command = Command::OpenPane { pane: "p1".into() };
            answer(
                &state,
                Ask::Drive {
                    device: None,
                    command,
                },
            )
        })
    };
    let Some(ServerMessage::Drive { id, drive }) = to_phone.blocking_recv() else {
        panic!("no drive was sent");
    };
    assert_eq!(
        drive,
        Drive::Open {
            to: Screen::Chat { pane: "p1".into() }
        }
    );
    // Another socket of the same phone cannot answer for the one asked.
    state.drivers.answer(6, id, done());
    state.drivers.answer(5, id, done());
    match asking.join().unwrap() {
        Answer::Driven { phone, outcome } => {
            assert_eq!(phone.name, "iPhone");
            assert_eq!(outcome, done());
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_drive_unanswered_in_time_or_sent_to_a_closed_socket_says_so() {
    let drivers = Drivers::default();
    let (out, _to_phone) = mpsc::channel(4);
    drivers.allow(1, Some((DeviceId("d".into()), out)), 0);
    let wait = Duration::from_millis(50);
    assert_eq!(drivers.drive(1, Drive::State, wait), None);
    assert_eq!(drivers.drive(2, Drive::State, wait), None);
    assert!(drivers.inner().waiting.is_empty());
}

#[test]
fn a_socket_that_closes_ends_its_drives_at_once() {
    let drivers = std::sync::Arc::new(Drivers::default());
    let (out, mut to_phone) = mpsc::channel(4);
    drivers.allow(1, Some((DeviceId("d".into()), out)), 0);
    let started = Instant::now();
    let asking = {
        let drivers = drivers.clone();
        std::thread::spawn(move || drivers.drive(1, Drive::State, Duration::from_secs(30)))
    };
    assert!(to_phone.blocking_recv().is_some());
    {
        let _seat_of_socket_1 = Seat {
            drivers: &drivers,
            id: 1,
        };
    }
    assert_eq!(asking.join().unwrap(), None);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(drivers.phones(|_| Some("p".into())).is_empty());
}

#[test]
fn a_terminal_agent_the_phone_shows_as_a_chat_opens_as_its_chat() {
    let dir = tempfile::tempdir().unwrap();
    let mut layout = layout(EndpointId::random());
    let terminal = &mut layout.workspaces[0].terminals[0];
    terminal.agents = terminal.agent.clone().into_iter().collect();
    let state = offline(dir.path(), layout);
    let opened = resolve(&state, Command::OpenPane { pane: "p2".into() });
    assert!(state.hub.observable("p2"));
    let expected = Screen::Chat { pane: "p2".into() };
    assert_eq!(opened, Ok(Drive::Open { to: expected }));
}

#[test]
fn guidance_validates_text_and_routes_named_settings_without_a_pane() {
    use crate::control::phone::{Highlight, SettingsPage};
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    for text in [String::new(), " ".into(), "é".repeat(501)] {
        assert_eq!(
            resolve(
                &state,
                Command::Guide {
                    text,
                    highlight: Some(Highlight::PushRelay)
                }
            ),
            Err(Answer::Invalid)
        );
    }
    assert_eq!(
        resolve(
            &state,
            Command::Guide {
                text: "Push settings".into(),
                highlight: Some(Highlight::PushRelay)
            }
        ),
        Ok(Drive::Guide {
            text: "Push settings".into(),
            highlight: Some(Highlight::PushRelay)
        })
    );
    assert_eq!(
        resolve(
            &state,
            Command::Highlight {
                target: Highlight::PushRelay
            }
        ),
        Ok(Drive::Highlight {
            target: Highlight::PushRelay
        })
    );
    assert_eq!(resolve(&state, Command::ClearGuide), Ok(Drive::ClearGuide));
    for page in SettingsPage::ALL {
        let to = Screen::SettingsPage { page: *page };
        assert_eq!(
            resolve(&state, Command::Open { to: to.clone() }),
            Ok(Drive::Open { to })
        );
    }
}
