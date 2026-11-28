#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use sheprd_relay::Endpoint;
use sheprd_relay_directory::Relay;

/// A list the site workflow signed, from the relay list crate's own fixtures.
const SIGNED: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sheprd-relay-directory/src/tests/fixtures/relays-signed.json"
));
const SIGNED_SIG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sheprd-relay-directory/src/tests/fixtures/relays-signed.json.sig"
));
/// When the fixture was signed, in Unix seconds.
const SIGNED_AT: u64 = 1_794_506_300;

#[test]
fn a_signed_list_is_offered_with_who_vouched_for_it() {
    let signatures = Signatures {
        ed25519: Some(SIGNED_SIG),
        ..Signatures::default()
    };
    let listed = listed(SIGNED, signatures, SIGNED_AT, None).unwrap();
    assert_eq!(listed.signer, Signer::Actions);
    assert!(listed.relays.is_empty());
}

#[test]
fn a_list_nobody_signed_or_that_expired_offers_nothing() {
    let unsigned = listed(SIGNED, Signatures::default(), SIGNED_AT, None);
    assert!(matches!(
        unsigned,
        Err(Error::Refused(directory::Error::Unsigned))
    ));
    let signatures = Signatures {
        ed25519: Some(SIGNED_SIG),
        ..Signatures::default()
    };
    let expired = listed(SIGNED, signatures, 4_102_444_800, None);
    assert!(matches!(
        expired,
        Err(Error::Refused(directory::Error::Expired { .. }))
    ));
}

#[test]
fn each_relay_is_offered_by_the_address_the_daemon_names_it_by() {
    let listed = Listed::from(signed::Signed {
        directory: Directory {
            expires: Some(2),
            relays: vec![Relay {
                endpoint: Endpoint::parse("https://fra1.relay.sheprd.org/").unwrap(),
                region: Some("fra1".into()),
                name: Some("Frankfurt".into()),
                quic_pin: Some([7; 32]),
            }],
            serial: Some(1),
        },
        expires: 2,
        serial: 1,
        signer: Signer::Maintainer,
    });
    assert!(!listed.expired(1) && listed.expired(2));
    assert_eq!(listed.serial, 1);
    assert_eq!(
        listed.relays,
        [Public {
            address: "https://fra1.relay.sheprd.org".into(),
            name: Some("Frankfurt".into()),
            region: Some("fra1".into()),
            quic_pin: Some([7; 32]),
        }]
    );
}

fn failed() -> Result<Option<Vec<u8>>, Error> {
    Err(Error::Download(ureq::Error::ConnectionFailed))
}

#[test]
fn one_signature_failing_to_download_leaves_the_other_to_vouch() {
    let signature = || Ok(Some(SIGNED_SIG.to_vec()));
    let listed = settle(SIGNED, failed(), signature(), SIGNED_AT, None).unwrap();
    assert_eq!(listed.signer, Signer::Actions);
    // Neither arrived, or the one that did vouches for nothing: the failed
    // download is why, not a list nobody signed.
    for ed25519 in [failed(), Ok(None), Ok(Some(b"nonsense".to_vec()))] {
        let refused = settle(SIGNED, failed(), ed25519, SIGNED_AT, None);
        assert!(matches!(refused, Err(Error::Download(_))));
    }
    // Every download worked: the list itself is what is refused.
    let unsigned = settle(SIGNED, Ok(None), Ok(None), SIGNED_AT, None);
    assert!(matches!(
        unsigned,
        Err(Error::Refused(directory::Error::Unsigned))
    ));
    let expired = settle(SIGNED, failed(), signature(), 4_102_444_800, None);
    assert!(matches!(
        expired,
        Err(Error::Refused(directory::Error::Expired { .. }))
    ));
}

#[test]
fn a_list_older_than_the_newest_taken_is_refused() {
    let signatures = || Signatures {
        ed25519: Some(SIGNED_SIG),
        ..Signatures::default()
    };
    let serial = listed(SIGNED, signatures(), SIGNED_AT, None)
        .unwrap()
        .serial;
    // The same list again is still taken.
    assert!(listed(SIGNED, signatures(), SIGNED_AT, Some(serial)).is_ok());
    let stale = listed(SIGNED, signatures(), SIGNED_AT, Some(serial + 1));
    assert!(matches!(
        stale,
        Err(Error::Refused(directory::Error::Stale { .. }))
    ));
}

#[test]
fn the_newest_serial_is_kept_and_a_damaged_file_keeps_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = serial_path(dir.path());
    assert_eq!(kept_serial(&path), None);
    crate::state_file::write(&path, &1_794_506_367_u64).unwrap();
    assert_eq!(kept_serial(&path), Some(1_794_506_367));
    std::fs::write(&path, "not a serial").unwrap();
    assert_eq!(kept_serial(&path), None);
}
