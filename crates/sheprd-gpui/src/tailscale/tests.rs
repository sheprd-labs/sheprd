//! What the surfaces read: whether a connection is waiting on a Tailscale check,
//! and where that check came from. Both decide whether a person gets something
//! to click, so both are checked with a real address the client accepted, not a
//! stand-in.

#![allow(clippy::expect_used)]

// Named imports, not `super::*`: this module pulls `gpui` in with a glob, and a
// glob-imported macro of the same name takes a plain `#[test]` over.
use crate::tailscale::{check, pending};
use sheprd_client::{SshFailure, TailscaleCheck};

const URL: &str = "https://login.tailscale.com/a/abc123";

#[test]
fn a_typed_failure_names_its_check() {
    let failure = TailscaleCheck::from_address(URL).map(SshFailure::Tailscale);
    assert_eq!(
        check(failure.as_ref(), None).and_then(|check| check.url().map(str::to_owned)),
        Some(URL.to_owned())
    );
}

#[test]
fn a_sentence_names_the_check_the_transport_reported_as_text() {
    // The `PermissionDenied` path: no typed failure, and the refusal's sentence
    // carries the address. A surface that read only the failure would show a
    // message and nothing to act on.
    let sentence = format!(
        "SSH refused the connection: Tailscale SSH wants you to verify your identity: open {URL} to authenticate, then try again"
    );
    let found = check(None, Some(&sentence)).expect("the sentence names a check");
    assert_eq!(found.url(), Some(URL));
}

#[test]
fn a_typed_failure_wins_over_the_sentence_it_came_with() {
    let sentence = format!("Tailscale SSH wants you to verify your identity: open {URL}");
    let failure = TailscaleCheck::from_address(URL).map(SshFailure::Tailscale);
    let found = check(failure.as_ref(), Some(&sentence)).expect("a check is pending");
    assert_eq!(found.url(), Some(URL));
}

#[test]
fn a_sentence_naming_somebody_elses_address_is_no_check() {
    // Host text is remote text: an address that is not one of Tailscale's own
    // must not become a link the user would be asked to open.
    for sentence in [
        "open https://example.com/a/abc to authenticate",
        "sign in at https://login.tailscale.com.example/a/abc",
        "host unreachable",
    ] {
        assert_eq!(
            check(None, Some(sentence)),
            None,
            "{sentence} names no check"
        );
    }
}

#[test]
fn other_failures_offer_no_address() {
    // A refused identity is not a check to finish, and no failure at all is
    // neither: a surface must not draw a link from either.
    for failure in [None, Some(SshFailure::Auth), Some(SshFailure::NoRoute)] {
        assert_eq!(
            pending(failure.as_ref()),
            None,
            "{failure:?} names no address"
        );
    }
}
