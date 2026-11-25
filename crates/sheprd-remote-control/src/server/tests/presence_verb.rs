//! `presence`: Sheprd on the computer a phone is near marks the phone's
//! device near or away on this one, over SSH, so this daemon holds its
//! alerts too.

use super::{options::USAGE, *};
use crate::{app::presence, control::DeviceId};

#[test]
fn presence_takes_a_device_and_near_or_away() {
    assert!(matches!(
        parse(args(&["presence", "--device", "d", "--near"])).unwrap(),
        Invocation::Presence { state_dir: None, device, near: true } if device == "d"
    ));
    assert!(matches!(
        parse(args(&["presence", "--away", "--device", "d", "--state-dir", "/s"])).unwrap(),
        Invocation::Presence { state_dir: Some(dir), near: false, .. }
            if dir == std::path::Path::new("/s")
    ));
    assert!(matches!(
        parse(args(&["presence", "--device", "d"])),
        Err(ParseError::NoPresence)
    ));
    assert!(matches!(
        parse(args(&["presence", "--near"])),
        Err(ParseError::Missing("--device"))
    ));
    assert!(parse(args(&["presence", "--device", "d", "--near", "--away"])).is_err());
    assert!(parse(args(&["presence", "--device", "", "--near"])).is_err());
    assert!(USAGE.contains("presence --device ID --near|--away [--state-dir DIR]"));
}

#[test]
fn no_daemon_has_nothing_to_hold() {
    let dir = tempfile::tempdir().unwrap();
    presence::set(dir.path(), DeviceId("d".to_owned()), true).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_running_daemon_marks_the_device_near_then_away() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    let (device, _) = web::tests::signed_in(&state, 3);
    let (stop, _) = watch::channel(false);
    control::serve(dir.path(), state.clone(), stop).unwrap();
    let set = |device: DeviceId, near: bool| {
        let path = dir.path().to_owned();
        tokio::task::spawn_blocking(move || presence::set(&path, device, near))
    };

    set(device.clone(), true).await.unwrap().unwrap();
    assert!(state.near().is_near(&device, web::now_ms()));
    set(device.clone(), false).await.unwrap().unwrap();
    assert!(!state.near().is_near(&device, web::now_ms()));
    // A device this host does not know, such as one revoked here, is fine.
    set(DeviceId("gone".to_owned()), true)
        .await
        .unwrap()
        .unwrap();
}
