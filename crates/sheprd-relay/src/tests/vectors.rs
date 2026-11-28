//! The test vectors `PROTOCOL.md` points implementers at, in `vectors/`,
//! made from fixed keys, nonces, and secrets. Each run makes them again
//! and fails on any difference; `RELAY_UPDATE_VECTORS=1` rewrites them.
//!
//! ECDSA signatures here are deterministic (RFC 6979), but other
//! implementations may randomize theirs, so the files give signatures to
//! verify, never ones to reproduce.

use super::{public, sign};
use crate::{
    Endpoint, RouteId,
    framing::{self, MAX_MESSAGE},
    route::pairing_dial_key,
    socket::{self, FromListener, Hello, Id, ToPeer, code},
    wire::{self, Message, Request, Status},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::SigningKey;
use serde_json::{Value, json};
use std::path::PathBuf;

/// The listener's private scalar: 0x11 repeated.
const LISTENER: [u8; 32] = [0x11; 32];
/// The dialer's private scalar: 0x22 repeated.
const DIALER: [u8; 32] = [0x22; 32];
/// The WebSocket challenge's nonce.
const NONCE: [u8; 32] = [0x4e; 32];
/// What a QUIC connection's TLS exporter gave, in place of a real one.
const EXPORTED: [u8; 32] = [0x5a; 32];
/// A pairing secret both sides hold.
const PAIRING_SECRET: [u8; 32] = [0x5e; 32];
/// The relay's name, with a path prefix.
const PROOF_NAME: &str = "relay.example.org/acme";

fn text(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn key(scalar: [u8; 32]) -> SigningKey {
    SigningKey::from_slice(&scalar).unwrap()
}

fn party(scalar: [u8; 32]) -> Value {
    let key = key(scalar);
    let route = RouteId::of(key.verifying_key());
    json!({
        "private": hex(&scalar),
        "public": hex(&public(&key)),
        "public_text": b64(&public(&key)),
        "route": hex(route.as_bytes()),
        "route_text": route.to_string(),
    })
}

fn check(name: &str, value: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vectors")
        .join(format!("{name}.json"));
    let text = format!("{}\n", serde_json::to_string_pretty(value).unwrap());
    if std::env::var_os("RELAY_UPDATE_VECTORS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let expected: Value = serde_json::from_str(&expected).unwrap();
    assert_eq!(
        value,
        &expected,
        "{} is out of date; run with RELAY_UPDATE_VECTORS=1",
        path.display()
    );
}

#[test]
fn routes() {
    check(
        "routes",
        &json!({
            "comment": "route = SHA-256(SEC1 compressed P-256 public key); route_text is base64url without padding",
            "keys": [party(LISTENER), party(DIALER)],
        }),
    );
}

#[test]
fn websocket_proofs() {
    let listener = key(LISTENER);
    let dialer = key(DIALER);
    let route = RouteId::of(listener.verifying_key());
    let listen_signed = socket::listen_signed(PROOF_NAME, &NONCE);
    let dial_signed = socket::dial_signed(PROOF_NAME, &NONCE, &route);
    let names: Vec<Value> = [
        "relay.example.org",
        "https://Relay.Example.org:443/",
        "wss://relay.example.org/acme/",
        "https://relay.example.org:8443/acme",
        "http://127.0.0.1:8080",
    ]
    .into_iter()
    .map(|text| {
        let endpoint = Endpoint::parse(text).unwrap();
        json!({
            "endpoint": text,
            "proof_name": endpoint.proof_name(),
            "listen": endpoint.listen().as_str(),
            "dial": endpoint.dial(&route).as_str(),
            "info": endpoint.info().as_str(),
        })
    })
    .collect();
    let hello = |key: &SigningKey, signed: &[u8], token: Option<&str>| {
        serde_json::to_string(&Hello {
            key: b64(&public(key)),
            signature: b64(&sign(key, signed)),
            token: token.map(str::to_owned),
        })
        .unwrap()
    };
    check(
        "websocket",
        &json!({
            "comment": "signed = context || proof name (at most 255 bytes) || 0x00 || nonce || extra; signatures are r || s and must verify, not be reproduced",
            "endpoints": names,
            "proof_name": PROOF_NAME,
            "nonce": hex(&NONCE),
            "nonce_text": b64(&NONCE),
            "listener": party(LISTENER),
            "dialer": party(DIALER),
            "listen_context": hex(socket::LISTEN_CONTEXT),
            "dial_context": hex(socket::DIAL_CONTEXT),
            "listen_signed": hex(&listen_signed),
            "listen_signature": hex(&sign(&listener, &listen_signed)),
            "listen_hello": hello(&listener, &listen_signed, Some("a listen token")),
            "dial_signed": hex(&dial_signed),
            "dial_signature": hex(&sign(&dialer, &dial_signed)),
            "dial_hello": hello(&dialer, &dial_signed, None),
        }),
    );
}

#[test]
fn quic_proofs() {
    let listener = key(LISTENER);
    let dialer = key(DIALER);
    let route = RouteId::of(listener.verifying_key());
    let listen_signed = wire::listen_signed(&EXPORTED);
    let dial_signed = wire::dial_signed(&EXPORTED, &route);
    let listen_signature = sign(&listener, &listen_signed);
    let dial_signature = sign(&dialer, &dial_signed);
    let listen = Request::Listen {
        key: public(&listener),
        signature: listen_signature,
        token: b"a listen token".to_vec(),
    };
    let dial = Request::Dial {
        key: public(&dialer),
        route,
        signature: dial_signature,
    };
    let dial_keys = Message::DialKeys(vec![public(&dialer)]);
    let pairing = Message::PairingKey {
        key: public(&dialer),
        seconds: 300,
    };
    let incoming = [&[wire::INCOMING][..], &public(&dialer)].concat();
    check(
        "quic",
        &json!({
            "comment": "exported stands in for the 32 bytes the connection's TLS exporter gives (RFC 5705, label EXPORTER-sheprd-relay-listen, no context); signatures must verify, not be reproduced",
            "alpn": String::from_utf8_lossy(wire::ALPN),
            "exporter_label": String::from_utf8_lossy(wire::EXPORTER_LABEL),
            "exported": hex(&EXPORTED),
            "listener": party(LISTENER),
            "dialer": party(DIALER),
            "listen_context": hex(wire::LISTEN_CONTEXT),
            "dial_context": hex(wire::DIAL_CONTEXT),
            "listen_signed": hex(&listen_signed),
            "listen_signature": hex(&listen_signature),
            "listen_request": hex(&listen.encode().unwrap()),
            "dial_signed": hex(&dial_signed),
            "dial_signature": hex(&dial_signature),
            "dial_request": hex(&dial.encode().unwrap()),
            "dial_keys_message": hex(&dial_keys.encode().unwrap()),
            "pairing_key_message": hex(&pairing.encode().unwrap()),
            "incoming_header": hex(&incoming),
        }),
    );
}

#[test]
fn pairing() {
    let key = pairing_dial_key(&PAIRING_SECRET);
    check(
        "pairing",
        &json!({
            "comment": "scalar = the first SHA-256(context || secret || counter byte), counter from 0, that is a valid P-256 scalar",
            "context": hex(b"sheprd-relay/pairing-dial/v1\0"),
            "secret": hex(&PAIRING_SECRET),
            "scalar": hex(&key.to_bytes()),
            "public": hex(&public(&key)),
            "public_text": b64(&public(&key)),
            "route": hex(RouteId::of(key.verifying_key()).as_bytes()),
        }),
    );
}

#[test]
fn framing() {
    let cases: Vec<Value> = [
        vec![b"".to_vec()],
        vec![b"hello".to_vec()],
        vec![b"one".to_vec(), vec![0xab; 258]],
    ]
    .into_iter()
    .map(|messages| {
        let encoded: Vec<u8> = messages
            .iter()
            .flat_map(|message| framing::encode(message).unwrap())
            .collect();
        json!({
            "messages": messages.iter().map(|message| hex(message)).collect::<Vec<_>>(),
            "encoded": hex(&encoded),
        })
    })
    .collect();
    check(
        "framing",
        &json!({
            "comment": "each message is a big-endian u16 length, then the message; transport frames may split or join messages anywhere",
            "max_message": MAX_MESSAGE,
            "cases": cases,
        }),
    );
}

/// The exact JSON each WebSocket control message has, as this relay
/// writes or reads it, and every status and close code.
#[test]
fn messages() {
    let dialer = b64(&public(&key(DIALER)));
    let route = RouteId::of(key(LISTENER).verifying_key()).to_string();
    let statuses: Vec<Value> = (0..=u8::MAX)
        .filter_map(Status::from_byte)
        .map(|status| json!({"byte": status as u8, "json": serde_json::to_value(status).unwrap()}))
        .collect();
    check(
        "messages",
        &json!({
            "comment": "the relay's and listener's JSON text frames; \"id\" is optional and older relays neither read nor echo it",
            "challenge": text(&ToPeer::Challenge { nonce: b64(&NONCE), version: 1 }),
            "ready": text(&ToPeer::Ready { route }),
            "incoming": text(&ToPeer::Incoming { key: dialer.clone(), session: "AAAAAAAAAAAAAAAAAAAAAA".into() }),
            "status": text(&ToPeer::Status { id: None, status: Status::Ok }),
            "status_with_id": text(&ToPeer::Status { id: Some(Id::Number(7)), status: Status::TooMany }),
            "dial_keys": text(&FromListener::DialKeys { id: None, keys: vec![dialer.clone()] }),
            "dial_keys_with_id": text(&FromListener::DialKeys { id: Some(Id::Text("k1".into())), keys: vec![dialer.clone()] }),
            "pairing_key": text(&FromListener::PairingKey { id: None, key: dialer.clone(), seconds: 300 }),
            "pairing_key_with_id": text(&FromListener::PairingKey { id: Some(Id::Number(8)), key: dialer, seconds: 300 }),
            "statuses": statuses,
            "close_codes": {
                "peer_gone": code::PEER_GONE,
                "protocol": code::PROTOCOL,
                "refused": code::REFUSED,
                "revoked": code::REVOKED,
                "offline": code::OFFLINE,
                "idle": code::IDLE,
                "replaced": code::REPLACED,
                "expired": code::EXPIRED,
                "banned": code::BANNED,
                "full": code::FULL,
            },
            "quic_codes": {
                "done": wire::code::DONE,
                "peer_gone": wire::code::PEER_GONE,
                "idle": wire::code::IDLE,
                "expired": wire::code::EXPIRED,
                "protocol": wire::code::PROTOCOL,
                "replaced": wire::code::REPLACED,
                "shutdown": wire::code::SHUTDOWN,
                "revoked": wire::code::REVOKED,
                "banned": wire::code::BANNED,
            },
        }),
    );
}
