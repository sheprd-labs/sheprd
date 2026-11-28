//! What the relay list may point clients at: public hosts only, never
//! their own machine or network, and pins that are pins.

use crate::{Directory, Error};

fn list(endpoint: &str) -> Result<Directory, Error> {
    Directory::parse(
        format!(r#"{{"version":1,"relays":[{{"endpoint":"{endpoint}"}}]}}"#).as_bytes(),
    )
}

#[test]
fn a_list_naming_a_private_or_reserved_host_is_refused() {
    for endpoint in [
        "https://127.0.0.1",
        "https://10.0.0.5",
        "https://192.168.1.2:8443",
        "https://169.254.169.254",
        "https://100.64.0.1",
        "https://0.0.0.0",
        "https://[::1]",
        "https://[fd00::1]",
        "https://[fe80::1]",
        "https://localhost",
        "https://relay.local",
        "https://metadata.google.internal",
        "https://10.0.0.5.sslip.io",
        "https://relay.127.0.0.1.nip.io",
        "https://10-0-0-5.sslip.io",
        "https://a-192-168-0-1.nip.io",
    ] {
        assert!(
            matches!(list(endpoint), Err(Error::Private { index: 0 })),
            "{endpoint}"
        );
    }
}

#[test]
fn a_list_naming_public_hosts_is_taken() {
    for endpoint in [
        "https://fra1.relay.sheprd.org",
        "https://8.8.8.8",
        "https://1-2-3-4.sslip.io",
        "https://[2606:4700::1111]",
        "https://relay.example.org/sheprd",
    ] {
        assert!(list(endpoint).is_ok(), "{endpoint}");
    }
}

#[test]
fn a_relay_may_carry_its_quic_pin() {
    let pin = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA";
    let text = format!(
        r#"{{"version":1,"relays":[{{"endpoint":"https://8.8.8.8","quic_pin":"{pin}"}}]}}"#
    );
    let directory = Directory::parse(text.as_bytes()).unwrap();
    let expected: Vec<u8> = (1..=32).collect();
    assert_eq!(directory.relays[0].quic_pin.unwrap().to_vec(), expected);
    let short = r#"{"version":1,"relays":[{"endpoint":"https://8.8.8.8","quic_pin":"AQID"}]}"#;
    assert!(matches!(
        Directory::parse(short.as_bytes()),
        Err(Error::Pin { index: 0 })
    ));
}
