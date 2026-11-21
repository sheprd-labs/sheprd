use super::*;
use sheprd_remote_control::control::push::{Mode, PushStatus};

#[test]
fn a_push_status_poll_reports_only_what_changed() {
    let mut supervisor = Supervisor::default();
    let relay = || {
        Ok(Response::PushStatus(PushStatus {
            mode: Mode::PushRelay,
            ..PushStatus::default()
        }))
    };
    assert!(supervisor.push_polled(relay()));
    assert!(!supervisor.push_polled(relay()));
    assert_eq!(
        supervisor.push_status.as_ref().map(|status| status.mode),
        Some(Mode::PushRelay)
    );
    // A daemon too old to answer leaves it unknown rather than off.
    assert!(supervisor.push_polled(Err(control::Error::NoAnswer)));
    assert_eq!(supervisor.push_status, None);
    assert!(!supervisor.push_polled(Ok(Response::Done)));
}
