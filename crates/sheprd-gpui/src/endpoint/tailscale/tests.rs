//! The check an endpoint keeps across the retries that empty its live error.

#![allow(clippy::unwrap_used)]

use super::{LiveState, PendingCheck};
use sheprd_client::{SshFailure, TailscaleCheck};

const URL: &str = "https://login.tailscale.com/a/o84652-335960037";

/// The sentence a `PermissionDenied` refusal leaves: no typed failure at all.
fn refusal_sentence() -> String {
    format!(
        "Tailscale SSH wants you to verify your identity: open {URL} to authenticate, then try again"
    )
}

fn named(url: &str) -> SshFailure {
    SshFailure::Tailscale(TailscaleCheck::from_address(url).unwrap())
}

fn state(failure: Option<SshFailure>, error: Option<String>) -> LiveState {
    let mut live = LiveState::default();
    live.ssh_failure = failure;
    live.error = error;
    live
}

#[test]
fn a_typed_failure_is_pending_at_once() {
    let mut check = PendingCheck::default();
    let live = state(Some(named(URL)), None);
    check.observe(&live, None);
    assert_eq!(
        check.pending(&live).and_then(TailscaleCheck::url),
        Some(URL)
    );
}

#[test]
fn a_sentence_is_read_when_the_transport_reports_no_failure() {
    // The transport reports `PermissionDenied`, so the address reaches the
    // window as text: a surface reading only the typed failure would never
    // offer the one thing that finishes the check.
    let mut check = PendingCheck::default();
    let live = state(None, Some(refusal_sentence()));
    check.observe(&live, None);
    assert_eq!(
        check.pending(&live).and_then(TailscaleCheck::url),
        Some(URL)
    );
}

#[test]
fn the_check_outlives_an_outage_sentence_that_does_not_name_it() {
    let mut check = PendingCheck::default();
    check.observe(&state(None, Some(refusal_sentence())), None);
    // The drop recorded its own reason while the check was still pending. It
    // says nothing about the check, so the address stays on screen.
    let retrying = LiveState::default();
    check.observe(&retrying, Some("the host stopped answering"));
    assert_eq!(
        check.pending(&retrying).and_then(TailscaleCheck::url),
        Some(URL),
        "the address is remembered, not re-read from the last attempt"
    );
}

#[test]
fn an_address_that_is_not_tailscales_is_never_remembered() {
    let mut check = PendingCheck::default();
    let live = state(
        None,
        Some("open https://example.com/a/abc to authenticate".to_owned()),
    );
    check.observe(&live, None);
    assert_eq!(check.pending(&live), None, "host text is not a link");
}

#[test]
fn a_connected_host_forgets_the_check() {
    let mut check = PendingCheck::default();
    check.observe(&state(None, Some(refusal_sentence())), None);
    check.clear();
    assert_eq!(check.pending(&LiveState::default()), None);
}

#[test]
fn another_failure_ends_the_check() {
    // A retry that comes back refused for a different reason is the answer now:
    // keeping the old check would leave the prompt up over the error the host
    // is actually reporting, and its retry.
    for failure in [SshFailure::Auth, SshFailure::Unreachable, SshFailure::Other] {
        let mut check = PendingCheck::default();
        check.observe(&state(None, Some(refusal_sentence())), None);
        assert!(check.pending(&state(None, None)).is_some());
        let answered = state(
            Some(failure.clone()),
            Some("SSH refused the connection".to_owned()),
        );
        check.observe(&answered, None);
        assert_eq!(check.pending(&answered), None, "{failure:?} ends the check");
    }
}

#[test]
fn a_retry_that_has_not_answered_yet_keeps_the_check() {
    // The retry loop replaces `live` with nothing at all before the worker says
    // anything, and that must not take the address off the screen.
    let mut check = PendingCheck::default();
    check.observe(&state(None, Some(refusal_sentence())), None);
    check.observe(&LiveState::default(), None);
    assert_eq!(
        check
            .pending(&LiveState::default())
            .and_then(TailscaleCheck::url),
        Some(URL)
    );
}
