//! A finished turn's pull request: sealed only as a GitHub pull request
//! address, dropped before the push fails to fit, and the fixture the
//! Swift package opens.

use super::alert::{COMPUTER, fixed_seal, json, open, push, question};
use crate::apns::{alert::Sealed, payload::MAX_PAYLOAD};
use crate::push::{Kind, Notification, Urgency};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::path::PathBuf;

const PR: &str = "https://github.com/sheprd-labs/sheprd/pull/1642";

fn finished(pr: Option<&str>) -> Notification {
    Notification {
        body: "Added the Open PR action to finished pushes.".to_owned(),
        kind: Kind::TurnEnded,
        permission: None,
        title: "Claude finished".to_owned(),
        urgency: Urgency::Normal,
        ..question()
    }
    .with_pr(pr.map(str::to_owned))
}

#[test]
fn a_finished_turn_seals_its_pull_request() {
    let sealed = Sealed::new(&finished(Some(PR)), COMPUTER.to_owned(), 0);
    assert_eq!(sealed.pr.as_deref(), Some(PR));
    let sent = push(sealed, "chat-w1:p2", fixed_seal).unwrap();
    let text = String::from_utf8(sent.body.clone()).unwrap();
    assert!(!text.contains("github.com"), "only inside the seal");
    let opened = json(&open(json(&sent.body)["sealed"].as_str().unwrap()));
    assert_eq!(opened["pr"], PR);
    // Without one, the key is left out, as older phones expect.
    let plain = Sealed::new(&finished(None), COMPUTER.to_owned(), 0);
    assert!(serde_json::to_value(&plain).unwrap().get("pr").is_none());
}

#[test]
fn only_a_finished_turn_carries_a_pull_request() {
    let asked = question().with_pr(Some(PR.to_owned()));
    assert_eq!(asked.pr, None);
    assert_eq!(Sealed::new(&asked, COMPUTER.to_owned(), 0).pr, None);
}

#[test]
fn anything_but_a_github_pull_request_address_is_left_out() {
    for url in [
        "https://example.com/o/r/pull/7",
        "http://github.com/o/r/pull/7",
        "https://github.com/o/r/issues/7",
        "https://github.com/o/r/pull/7/files",
        "https://github.com/o/r/pull/7?x=1",
        "javascript:alert(1)",
        "",
    ] {
        let sealed = Sealed::new(&finished(Some(url)), COMPUTER.to_owned(), 0);
        assert_eq!(sealed.pr, None, "{url:?}");
    }
}

#[test]
fn the_pull_request_goes_before_the_push_does() {
    // The longest place that fits a push without one: no room for it.
    let crowded = |subtitle: usize, pr: Option<&str>| Notification {
        body: String::new(),
        subtitle: "é".repeat(subtitle),
        ..finished(pr)
    };
    let fits = |subtitle| {
        let sealed = Sealed::new(&crowded(subtitle, None), COMPUTER.to_owned(), 0);
        push(sealed, "chat-w1:p2", fixed_seal).is_ok()
    };
    let sizes: Vec<usize> = (0..2_000).collect();
    let longest = sizes.partition_point(|&subtitle| fits(subtitle)) - 1;
    let crowded = crowded(longest, Some(PR));
    let sent = push(
        Sealed::new(&crowded, COMPUTER.to_owned(), 0),
        "chat-w1:p2",
        fixed_seal,
    )
    .unwrap();
    assert!(sent.body.len() <= MAX_PAYLOAD, "{}", sent.body.len());
    let opened: Sealed =
        serde_json::from_slice(&open(json(&sent.body)["sealed"].as_str().unwrap())).unwrap();
    assert_eq!(opened.body, "");
    assert_eq!(opened.pr, None);
    // A long body alone is shortened and keeps it.
    let long = Notification {
        body: "é".repeat(5_000),
        ..finished(Some(PR))
    };
    let sent = push(
        Sealed::new(&long, COMPUTER.to_owned(), 0),
        "chat-w1:p2",
        fixed_seal,
    )
    .unwrap();
    let opened: Sealed =
        serde_json::from_slice(&open(json(&sent.body)["sealed"].as_str().unwrap())).unwrap();
    assert_eq!(opened.pr.as_deref(), Some(PR));
}

/// `Fixtures/notify/finished-pr.json`: a finished turn with its pull
/// request, which the Swift package opens. `HERDR_UPDATE_FIXTURES=1`
/// rewrites it.
#[test]
fn the_finished_pull_request_fixture_opens_to_its_plaintext() {
    let sealed = Sealed::new(&finished(Some(PR)), COMPUTER.to_owned(), 1);
    let sent = push(sealed.clone(), "chat-w1:p2", fixed_seal).unwrap();
    let fixture = json!({
        "device_private_key": URL_SAFE_NO_PAD.encode(super::alert::device().to_bytes()),
        "plaintext": sealed,
        "apns": json(&sent.body),
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../ios/SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/notify/finished-pr.json",
    );
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        let text = format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap());
        std::fs::write(&path, text).unwrap();
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        fixture,
        expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
    let opened = open(expected["apns"]["sealed"].as_str().unwrap());
    assert_eq!(opened, serde_json::to_vec(&sealed).unwrap());
}
