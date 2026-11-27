#![allow(clippy::unwrap_used)]

mod from_address;

use super::*;

/// What Tailscale's `HoldAndDelegate` action sends, as OpenSSH prints it at
/// its default `LogLevel`: the banner is verbatim from Tailscale's
/// `tailssh.AuthMessage`, with the address it builds.
const BANNER: &[u8] = b"Authenticated to node1.tailnet.ts.net ([100.1.2.3]:22) using \"none\".\r\n\
     Tailscale SSH requires an additional check to connect to alice@node1:\r\n\
    \x20 To authenticate, visit: https://login.tailscale.com/a/c2e8ee2fe1cf\r\n\r\n";

fn url_of(stderr: &[u8]) -> Option<String> {
    pending(stderr).and_then(|check| check.url)
}

#[test]
fn the_check_banner_is_recognised_with_the_address_to_open() {
    let check = pending(BANNER).unwrap();
    assert_eq!(
        check.url(),
        Some("https://login.tailscale.com/a/c2e8ee2fe1cf")
    );
    // What OpenSSH saw is not kept beyond the address.
    assert!(!format!("{check:?}").contains("alice"));
}

#[test]
fn either_half_of_the_banner_identifies_the_check() {
    // The address alone, as a wording change could leave it.
    assert_eq!(
        url_of(b"visit: https://login.tailscale.com/a/abc123\n"),
        Some("https://login.tailscale.com/a/abc123".into())
    );
    // The sentence alone, when the address could not be read: the run is
    // still explained, with no address to show.
    let check = pending(b"Tailscale SSH requires an additional check to connect to alice@node1:\n")
        .unwrap();
    assert_eq!(check.url(), None);
}

#[test]
fn an_unreadable_banner_still_says_what_it_can() {
    let check = pending(b"Tailscale SSH requires an additional check to connect to alice@node1:\n")
        .unwrap();
    let without = check.to_string();
    assert!(
        without.contains("connect to this host from a terminal"),
        "{without}"
    );
    assert!(!without.contains("https://"), "{without}");
    // With an address, the message hands it over instead.
    let with = pending(BANNER).unwrap().to_string();
    assert!(
        with.contains("open https://login.tailscale.com/a/c2e8ee2fe1cf"),
        "{with}"
    );
}

#[test]
fn only_tailscales_own_sign_in_host_is_offered_as_a_link() {
    // Any other origin is not Tailscale asking, whatever the banner claims.
    for stderr in [
        "To authenticate, visit: https://login.tailscale.example/a/abc123\n",
        "To authenticate, visit: https://evil.test/?next=https://login.tailscale.com/a/abc\n",
        "To authenticate, visit: ssh://login.tailscale.com/a/abc\n",
    ] {
        assert_eq!(pending(stderr.as_bytes()), None, "{stderr}");
    }
}

#[test]
fn a_token_that_could_not_be_a_token_yields_no_link() {
    // Anything that ends the token, or makes it longer than Tailscale's, is
    // dropped rather than truncated into a plausible-looking address.
    for stderr in [
        "https://login.tailscale.com/a/\n",
        "https://login.tailscale.com/a/abc/../etc\n",
        "https://login.tailscale.com/a/abc@evil.test\n",
        "https://login.tailscale.com/a/abc.def\n",
        "https://login.tailscale.com/a/abé\n",
        &format!(
            "https://login.tailscale.com/a/{}",
            "t".repeat(TOKEN_LIMIT + 1)
        ),
    ] {
        assert_eq!(url_of(stderr.as_bytes()), None, "{stderr}");
    }
    // The bound itself is allowed, and a long token is cut at the first
    // character a token cannot hold rather than swallowing what follows.
    assert_eq!(
        url_of(
            format!(
                "https://login.tailscale.com/a/{}\n",
                "t".repeat(TOKEN_LIMIT)
            )
            .as_bytes()
        ),
        Some(format!(
            "https://login.tailscale.com/a/{}",
            "t".repeat(TOKEN_LIMIT)
        ))
    );
    assert_eq!(
        url_of(b"https://login.tailscale.com/a/abc123 then more text\n"),
        Some("https://login.tailscale.com/a/abc123".into())
    );
}

#[test]
fn a_banner_still_arriving_waits_for_its_whole_address() {
    // The marker alone: the address is in a later read, so nothing is reported
    // yet and a surface never offers a check with nothing to open.
    assert_eq!(
        pending_live(b"Tailscale SSH requires an additional check to connect to alice@node1:\n"),
        None
    );
    // An address running to the end of what has arrived may still be growing.
    assert_eq!(
        pending_live(b"visit: https://login.tailscale.com/a/abc"),
        None
    );
    // Once a boundary arrives, the address is whole and is reported.
    assert_eq!(
        pending_live(b"visit: https://login.tailscale.com/a/abc\n")
            .and_then(|check| check.url().map(str::to_owned)),
        Some("https://login.tailscale.com/a/abc".into())
    );
    // Output that has finished is read as it is, where the address may be the
    // last thing the host managed to write.
    assert_eq!(
        url_of(b"visit: https://login.tailscale.com/a/abc"),
        Some("https://login.tailscale.com/a/abc".into())
    );
}

#[test]
fn a_link_is_found_after_the_first_that_is_not_one() {
    // A banner that prints a rejected token first still gets the real one.
    assert_eq!(
        url_of(
            b"see https://login.tailscale.com/a/bad.token\nhttps://login.tailscale.com/a/real1\n"
        ),
        Some("https://login.tailscale.com/a/real1".into())
    );
}

#[test]
fn ordinary_ssh_output_is_not_a_check() {
    for stderr in [
        "Permission denied, please try again.\n",
        "Host key verification failed.\n",
        "debug1: Authentications that can continue: publickey\n",
        "",
        // The marker's own words, without the sentence Tailscale writes.
        "tailscale ssh requires an additional check\n",
    ] {
        assert_eq!(pending(stderr.as_bytes()), None, "{stderr:?}");
    }
}

#[test]
fn the_address_is_redacted_where_a_failure_is_printed_for_logs() {
    let check = pending(BANNER).unwrap();
    let shown = format!("{check:?}");
    assert!(shown.contains("<authentication address>"), "{shown}");
    assert!(
        !shown.contains("c2e8ee2fe1cf"),
        "the token must not reach a log: {shown}"
    );
    // It is kept for the user who is being asked to authenticate.
    assert!(check.url().unwrap().contains("c2e8ee2fe1cf"));
}

#[test]
fn a_check_is_compared_by_its_address() {
    let one = pending(BANNER).unwrap();
    let same = pending(BANNER).unwrap();
    let other = pending(b"Tailscale SSH requires an additional check\n").unwrap();
    assert_eq!(one, same);
    assert_ne!(one, other);
}
