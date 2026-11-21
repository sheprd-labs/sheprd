//! `Request::PushStatus`: the mode and the last outcome of each kind, and
//! nothing that names a phone.

use super::*;
use crate::{
    apns::{Kind, Sent},
    control::{
        Request, Response,
        push::{Last, Mode, Outcome, PushStatus},
    },
};

#[test]
fn push_status_reports_the_mode_and_each_kinds_last_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    let (stop, _) = watch::channel(false);
    assert_eq!(
        control::answer(&state, &stop, Request::PushStatus),
        Response::PushStatus(PushStatus::default())
    );

    state.pushes.record(Kind::Alert, &Sent::Unreachable, 10);
    state.pushes.record(Kind::Alert, &Sent::Delivered, 20);
    state.pushes.record(
        Kind::LiveActivity,
        &Sent::Refused {
            status: 400,
            reason: Some("BadDeviceToken".repeat(10)),
        },
        30,
    );
    let Response::PushStatus(status) = control::answer(&state, &stop, Request::PushStatus) else {
        panic!("not a push status");
    };
    assert_eq!(status.mode, Mode::Off);
    assert_eq!(
        status.alert,
        Some(Last {
            at_ms: 20,
            outcome: Outcome::Delivered,
        })
    );
    let live = status.live_activity.unwrap();
    assert_eq!(live.at_ms, 30);
    assert!(!live.outcome.worked());
    let Outcome::Refused { status, reason } = live.outcome else {
        panic!("not refused");
    };
    assert_eq!(status, 400);
    assert_eq!(reason.unwrap().chars().count(), 64);
}

#[test]
fn each_delivery_names_its_mode() {
    assert_eq!(crate::apns::Delivery::Off.mode(), Mode::Off);
    assert_eq!(
        crate::apns::Delivery::OwnKey(Box::new(crate::apns::sender_for_tests())).mode(),
        Mode::OwnKey
    );
    assert_eq!(
        crate::apns::Delivery::Relay(Box::new(crate::apns::Relays::new().unwrap())).mode(),
        Mode::PushRelay
    );
}
