//! The whole path, as a computer and a phone run it: a computer listens
//! through [`crate::client`], a phone dials through a real relay, and both
//! seal what they send with a key only they hold. The seal here is a
//! stand-in for a real end-to-end protocol (a SHA-256 keystream), enough
//! to show the relay carries bytes it cannot read, untouched.

use super::*;
use crate::client::{self, Listener, public};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// One direction's keystream: SHA-256 of the secret, a direction byte, and
/// a block counter. Applying it twice gives the bytes back.
struct Seal {
    block: [u8; 32],
    counter: u64,
    direction: u8,
    secret: [u8; 32],
    used: usize,
}

impl Seal {
    fn new(secret: [u8; 32], direction: u8) -> Self {
        Self {
            block: [0; 32],
            counter: 0,
            direction,
            secret,
            used: 32,
        }
    }

    fn apply(&mut self, bytes: &[u8]) -> Vec<u8> {
        bytes
            .iter()
            .map(|byte| {
                if self.used == 32 {
                    self.block = Sha256::new()
                        .chain_update(self.secret)
                        .chain_update([self.direction])
                        .chain_update(self.counter.to_be_bytes())
                        .finalize()
                        .into();
                    self.counter += 1;
                    self.used = 0;
                }
                self.used += 1;
                byte ^ self.block[self.used - 1]
            })
            .collect()
    }
}

#[tokio::test]
async fn a_paired_phone_reaches_its_computer_through_the_relay_end_to_end() {
    let relay = start_both(&options(Limits::default())).await;
    let computer_relay_key = signing_key(170);
    let phone_dial_key = signing_key(171);
    let secret = [0x31; 32];

    let connection = client::connect(relay.quic, relay.pin).await.unwrap();
    let mut listener = Listener::listen(connection, &computer_relay_key, b"")
        .await
        .unwrap();
    listener
        .set_dial_keys(vec![public(&phone_dial_key)])
        .await
        .unwrap();
    let route = listener.route();

    let computer = tokio::spawn(async move {
        let mut incoming = listener.accept().await.unwrap();
        let (mut from_phone, mut to_phone) = (Seal::new(secret, 1), Seal::new(secret, 2));
        let mut request = [0; 13];
        incoming.recv.read_exact(&mut request).await.unwrap();
        assert_ne!(&request, b"GET /api/v2/\n");
        assert_eq!(from_phone.apply(&request), b"GET /api/v2/\n");
        incoming
            .send
            .write_all(&to_phone.apply(b"200 chats"))
            .await
            .unwrap();
        incoming.send.shutdown().await.unwrap();
        (listener, incoming)
    });

    let phone = client::connect(relay.quic, relay.pin).await.unwrap();
    let (mut send, mut recv) = client::dial(&phone, route, &phone_dial_key).await.unwrap();
    let (mut to_computer, mut from_computer) = (Seal::new(secret, 1), Seal::new(secret, 2));
    send.write_all(&to_computer.apply(b"GET /api/v2/\n"))
        .await
        .unwrap();
    let reply = recv.read_to_end(1024).await.unwrap();
    assert_eq!(from_computer.apply(&reply), b"200 chats");
    let _held = computer.await.unwrap();
    // The relay carried exactly what was sent, sealed, each way.
    let totals = relay.hub.metrics.totals();
    assert_eq!(totals.bytes_from_dialers, 13, "{totals:?}");
    assert_eq!(totals.bytes_from_listeners, 9, "{totals:?}");
}

#[tokio::test]
async fn a_dial_key_the_computer_did_not_name_is_refused_by_the_relay() {
    let relay = start_both(&options(Limits::default())).await;
    let computer_relay_key = signing_key(172);
    let connection = client::connect(relay.quic, relay.pin).await.unwrap();
    let mut listener = Listener::listen(connection, &computer_relay_key, b"")
        .await
        .unwrap();
    listener
        .set_dial_keys(vec![public(&signing_key(173))])
        .await
        .unwrap();
    let stranger = client::connect(relay.quic, relay.pin).await.unwrap();
    let refused = client::dial(&stranger, listener.route(), &signing_key(174)).await;
    assert!(matches!(
        refused,
        Err(client::Error::Refused(Status::Offline))
    ));
}
