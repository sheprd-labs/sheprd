//! The status bar's connection message, which the classic window and
//! Moonframe's status bar both show: silent about a Herdr 1 the host lacks
//! while its herdr-core runs the window's terminals.

use super::*;

/// A computer with no Herdr 1 installed, after the bridge failed to start one.
fn missing_installation() -> LiveState {
    let mut state = LiveState {
        missing_installation: true,
        ..Default::default()
    };
    state.apply(ClientEvent::Disconnected {
        reason: "Could not start herdr server".into(),
        ssh: None,
    });
    state
}

#[test]
fn a_connected_host_without_errors_says_nothing() {
    let state = LiveState {
        status: ConnectionStatus::Connected,
        ..Default::default()
    };
    assert_eq!(state.connection_message(None, false), None);
    assert_eq!(state.connection_message(None, true), None);
    // A local error still shows on a connected host.
    assert!(
        state
            .connection_message(Some("input failed"), true)
            .is_some()
    );
}

#[test]
fn no_herdr_1_says_nothing_while_the_core_answers() {
    let state = missing_installation();
    assert_eq!(state.connection_message(None, true), None);
    assert_eq!(state.connection_message(Some("old error"), true), None);
    // With no core either, nothing runs the terminals: the outage shows.
    let message = state.connection_message(None, false);
    assert!(
        message
            .as_deref()
            .is_some_and(|text| text.contains("Could not start herdr server")),
        "{message:?}"
    );
}

#[test]
fn a_herdr_1_the_user_runs_reports_its_disconnection_with_a_core() {
    let mut state = LiveState::default();
    state.apply(ClientEvent::Disconnected {
        reason: "socket closed".into(),
        ssh: None,
    });
    assert_eq!(
        state.connection_message(None, true),
        Some(state.status_text(None))
    );
}
