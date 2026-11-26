use super::*;

#[test]
fn a_running_macos_machine_is_asked_to_save_once() {
    let macos = MachineCapabilities::macos();
    assert_eq!(
        step(&Status::Running, macos, false, None, false),
        Step::Suspend
    );
    // Its save failed and left it running: it must not hold the quit forever.
    assert_eq!(step(&Status::Running, macos, true, None, false), Step::Done);
}

#[test]
fn a_machine_on_its_way_up_is_waited_for_then_saved() {
    let macos = MachineCapabilities::macos();
    assert_eq!(
        step(&Status::Starting, macos, false, None, false),
        Step::Wait
    );
    assert_eq!(
        step(&Status::Running, macos, false, None, false),
        Step::Suspend
    );
}

#[test]
fn a_saving_machine_holds_the_quit() {
    let macos = MachineCapabilities::macos();
    assert_eq!(step(&Status::Saving, macos, true, None, false), Step::Wait);
}

#[test]
fn a_stopped_or_failed_machine_lets_the_quit_go() {
    let macos = MachineCapabilities::macos();
    let linux = MachineCapabilities::linux_apple();
    assert_eq!(step(&Status::Stopped, macos, true, None, false), Step::Done);
    assert_eq!(
        step(&Status::Failed("no".to_owned()), linux, false, None, false),
        Step::Done
    );
}

#[test]
fn a_linux_machine_without_save_restore_shuts_down_gracefully_then_force_stops() {
    let linux = MachineCapabilities::linux_apple();
    assert_eq!(
        step(&Status::Running, linux, false, None, false),
        Step::ShutDown
    );
    assert_eq!(
        step(
            &Status::Running,
            linux,
            false,
            Some(Duration::from_secs(2)),
            false
        ),
        Step::Wait
    );
    assert_eq!(
        step(&Status::Running, linux, false, Some(SHUTDOWN_GRACE), false),
        Step::ForceStop
    );
    // If force-stop failed and left it running, do not hold app quit forever.
    assert_eq!(
        step(&Status::Running, linux, false, Some(SHUTDOWN_GRACE), true),
        Step::Done
    );
}

#[test]
fn a_machine_without_save_or_graceful_shutdown_force_stops_once() {
    let minimal = MachineCapabilities {
        save_restore: false,
        graceful_shutdown: false,
        dynamic_display: false,
    };
    assert_eq!(
        step(&Status::Running, minimal, false, None, false),
        Step::ForceStop
    );
    assert_eq!(
        step(&Status::Running, minimal, false, None, true),
        Step::Done
    );
}
