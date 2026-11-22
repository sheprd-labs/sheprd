//! `remote test-push`: one line per phone, and its exit code.

use super::*;
use crate::control::client::exit;
use sheprd_remote_control::control::push::{Mode, Outcome, TestPush};

fn push(device: &str, outcome: Outcome) -> TestPush {
    TestPush {
        device: device.into(),
        outcome,
    }
}

#[test]
fn each_phone_gets_a_line_and_a_failure_fails_the_command() {
    let output = remote::output(&Reply::TestPush {
        mode: Mode::PushRelay,
        pushes: vec![
            push("Work iPhone", Outcome::Delivered),
            push(
                "Old iPhone",
                Outcome::Refused {
                    status: 403,
                    reason: None,
                },
            ),
        ],
    })
    .unwrap();
    assert_eq!(
        output.text,
        "Work iPhone: Apple took it; it should show on the phone now\n\
         Old iPhone: refused (403)"
    );
    assert_eq!(output.json["pushes"][0]["device"], "Work iPhone");
    assert_eq!(output.json["pushes"][1]["status"], 403);
    assert_eq!(output.status, exit::REFUSED);

    let output = remote::output(&Reply::TestPush {
        mode: Mode::PushRelay,
        pushes: vec![push("Work iPhone", Outcome::Delivered)],
    })
    .unwrap();
    assert_eq!(output.status, 0);
}

#[test]
fn nothing_sent_says_why_and_exits_nothing() {
    let off = remote::output(&Reply::TestPush {
        mode: Mode::Off,
        pushes: Vec::new(),
    })
    .unwrap();
    assert!(off.text.contains("Turn on the push relay"), "{}", off.text);
    assert_eq!(off.status, exit::NOTHING);

    let none = remote::output(&Reply::TestPush {
        mode: Mode::PushRelay,
        pushes: Vec::new(),
    })
    .unwrap();
    assert!(none.text.contains("no paired iPhone"), "{}", none.text);
    assert_eq!(none.status, exit::NOTHING);
}
