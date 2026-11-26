use super::*;

#[test]
fn a_terminal_read_failure_is_an_error_not_a_quit() {
    let failed = terminal_event(Ok(Err(std::io::Error::other("tty gone"))));
    assert!(matches!(failed, Err(crate::Error::TerminalInput(_))));
}

#[test]
fn a_closed_terminal_input_is_an_error_not_a_quit() {
    let closed = terminal_event(Err(crossbeam_channel::RecvError));
    assert!(matches!(
        closed,
        Err(crate::Error::TerminalInput(source)) if source.kind() == std::io::ErrorKind::UnexpectedEof
    ));
}

#[test]
fn a_terminal_event_passes_through() {
    assert!(matches!(
        terminal_event(Ok(Ok(Event::FocusGained))),
        Ok(Event::FocusGained)
    ));
}

#[test]
fn frame_interval_slows_down_while_a_native_client_is_attached() {
    assert_eq!(frame_interval("idle", 0), None);
    assert_eq!(frame_interval("idle", 1), None);
    assert_eq!(frame_interval("working", 0), Some(FRAME));
    assert_eq!(frame_interval("working", 1), Some(ATTACHED_FRAME));
}
