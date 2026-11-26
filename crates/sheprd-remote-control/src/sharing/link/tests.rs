use crate::sharing::{
    Capability, Error, GrantId, Invitation, ShareId,
    gateway::Offer,
    link::{decode, encode},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use secrecy::{ExposeSecret, SecretBox, SecretString};
use sheprd_relay::{Endpoint, RouteId};
use sheprd_tunnel::Keys;
use uuid::Uuid;

fn fixture() -> (Endpoint, SecretString) {
    let relay = Endpoint::parse("https://relay.example:8443/private").unwrap();
    let invite = Invitation {
        share: ShareId(Uuid::new_v4()),
        grant: GrantId(Uuid::new_v4()),
        capability: Capability::from_bytes([17; 32]),
        expires_ms: 1234,
    };
    let offer = Offer {
        route: RouteId::from_bytes([23; 32]),
        identity: Keys::generate().unwrap().identity(),
        bootstrap: SecretBox::new(Box::new([42; 32])),
    };
    let link = encode(&relay, &invite, &offer).unwrap();
    (relay, link)
}

#[test]
fn only_the_exact_trusted_landing_page_accepts_the_fragment() {
    let (relay, link) = fixture();
    let (page, fragment) = link.expose_secret().split_once('#').unwrap();
    assert_eq!(page, "https://relay.example:8443/private/share/");
    assert!(decode(link.expose_secret(), &relay, 1233).is_ok());
    assert!(matches!(
        decode(link.expose_secret(), &relay, 1234),
        Err(Error::Denied)
    ));
    for page in [
        "https://relay.example:8443/share/",
        "https://relay.example:8443/private/share/?q",
        "https://relay.example:8443/private/share/../share/",
    ] {
        assert!(matches!(
            decode(&format!("{page}#{fragment}"), &relay, 0),
            Err(Error::InvalidInvitation)
        ));
    }
    let other = Endpoint::parse("https://second.example/private").unwrap();
    assert!(matches!(
        decode(&format!("{other}/share/#{fragment}"), &other, 0),
        Err(Error::InvalidInvitation)
    ));
    let local = Endpoint::parse("http://localhost").unwrap();
    let (invite, offer) = decode(link.expose_secret(), &relay, 0).unwrap();
    assert!(matches!(
        encode(&local, &invite, &offer),
        Err(Error::InvalidInvitation)
    ));
    assert!(matches!(
        decode(&"a".repeat(8193), &relay, 0),
        Err(Error::Limit)
    ));
}

#[test]
fn malformed_fields_and_future_versions_are_rejected_without_echoing_input() {
    let (relay, link) = fixture();
    let (page, fragment) = link.expose_secret().split_once('#').unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(fragment).unwrap()).unwrap();
    for (field, value) in [
        ("identity", "x".to_owned()),
        ("route", "x".to_owned()),
        ("bootstrap", URL_SAFE_NO_PAD.encode([0; 32])),
        ("capability", URL_SAFE_NO_PAD.encode([1; 31])),
        ("share", Uuid::nil().to_string()),
        ("sensitive text", "private".to_owned()),
    ] {
        let mut fields = original.clone();
        fields[field] = value.into();
        let fragment = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&fields).unwrap());
        assert!(matches!(
            decode(&format!("{page}#{fragment}"), &relay, 0),
            Err(Error::InvalidInvitation)
        ));
    }
    let mut fields = original;
    fields["version"] = 1.into();
    let fragment = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&fields).unwrap());
    assert!(matches!(
        decode(&format!("{page}#{fragment}"), &relay, 0),
        Err(Error::Version)
    ));
}
