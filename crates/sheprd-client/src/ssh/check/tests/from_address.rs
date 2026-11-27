//! [`TailscaleCheck::from_address`]: the only way to build a check from an
//! address alone, so it must apply the same proof a banner gets. A surface that
//! renders the address makes it clickable, so an address this accepted from
//! anywhere would be a link to open, on a click, that no Tailscale served.

use crate::TailscaleCheck;

#[test]
fn a_tailscale_sign_in_address_builds_a_check() {
    assert_eq!(
        TailscaleCheck::from_address("https://login.tailscale.com/a/abc123")
            .as_ref()
            .and_then(TailscaleCheck::url),
        Some("https://login.tailscale.com/a/abc123"),
    );
}

#[test]
fn an_address_from_elsewhere_builds_nothing() {
    // The scheme and the host are the whole claim; neither may be borrowed from
    // a banner, a config file, or a page.
    for url in [
        "https://evil.example/a/abc123",
        "http://login.tailscale.com/a/abc123",
        "https://login.tailscale.com.evil.example/a/abc123",
        "login.tailscale.com/a/abc123",
        "",
    ] {
        assert!(
            TailscaleCheck::from_address(url).is_none(),
            "{url} is not one of Tailscale's sign-in addresses"
        );
    }
}

#[test]
fn an_address_inside_other_text_builds_nothing() {
    // Unlike a banner, the caller's whole string is the address here, so a
    // look-alike that merely mentions Tailscale names nothing.
    for url in [
        "?next=https://login.tailscale.com/a/abc123",
        "https://login.tailscale.com/a/abc123?next=evil.example",
        "https://login.tailscale.com/a/bad.token",
    ] {
        assert!(
            TailscaleCheck::from_address(url).is_none(),
            "{url} is not an address on its own"
        );
    }
}
