use crate::sharing::{
    Capability, Content, History, Invitation,
    gateway::{
        Offer, Route, io,
        peer::{Peer, tunnel},
        proof, signing_key,
    },
    worker::{Owner, Snapshot},
};
use secrecy::ExposeSecret;
use sheprd_chat_protocol::{
    Cursor, EndpointId, Entry, EntryId, EntryKind, SessionInfo, Transcript,
    sharing::{MAX_REQUEST_BYTES, Update},
};
use sheprd_relay::client;
use sheprd_tunnel::Keys;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};

struct Session {
    _dir: tempfile::TempDir,
    owner: Owner,
    source: watch::Sender<Option<Snapshot>>,
    invite: Invitation,
    offer: Offer,
    route: Arc<Route>,
    connection: quinn::Connection,
    stop: watch::Sender<bool>,
    serving: tokio::task::JoinHandle<crate::sharing::Result<()>>,
}

impl Session {
    async fn new() -> Self {
        Self::with(None, Duration::from_secs(60)).await
    }

    async fn with(token: Option<&str>, lifetime: Duration) -> Self {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let (endpoint, info) =
            sheprd_relay::bind_quic("127.0.0.1:0".parse().unwrap(), None, None).unwrap();
        let quic = endpoint.local_addr().unwrap();
        let pin = URL_SAFE_NO_PAD
            .decode(&info.pin)
            .unwrap()
            .try_into()
            .unwrap();
        let (_, hub) = sheprd_relay::app(
            &sheprd_relay::Options {
                listen_token: token.map(|s| s.to_owned().into()),
                metrics: None,
                ..sheprd_relay::Options::default()
            },
            Some(info),
        )
        .unwrap();
        tokio::spawn(sheprd_relay::quic::run(endpoint, hub));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sharing");
        let owner = tokio::task::spawn_blocking(move || Owner::open(&path))
            .await
            .unwrap()
            .unwrap();
        let (source, feed) = watch::channel(Some(Snapshot {
            endpoint: EndpointId::random(),
            cursor: Cursor::new_epoch(),
            transcript: Arc::new(Transcript {
                agent: "Fixture agent".into(),
                session: Some(SessionInfo {
                    id: "session-A".into(),
                    cwd: Some("private-cwd".into()),
                    title: None,
                    continued: None,
                }),
                entries: vec![Entry {
                    id: EntryId(10),
                    kind: EntryKind::Agent {
                        text: "private old history".into(),
                    },
                }],
                ..Transcript::default()
            }),
        }));
        let share = owner
            .create(feed, History::FromNow, Content::default(), lifetime)
            .await
            .unwrap();
        let invite = owner.invite(share).await.unwrap();
        let route = Arc::new(Route::new().unwrap());
        let host = client::connect(quic, pin).await.unwrap();
        let listener = Arc::clone(&route)
            .listen(host, token.unwrap_or("").as_bytes(), owner.access(share))
            .await
            .unwrap();
        let (stop, stopped) = watch::channel(false);
        let serving = tokio::spawn(listener.run(stopped));
        let offer = route.offer(invite.grant).await.unwrap();
        let connection = client::connect(quic, pin).await.unwrap();
        Self {
            _dir: dir,
            owner,
            source,
            invite,
            offer,
            route,
            connection,
            stop,
            serving,
        }
    }

    async fn peer(&self) -> Peer {
        Peer::connect(
            &self.connection,
            &self.offer,
            self.invite.share,
            self.invite.grant,
            &self.invite.capability,
        )
        .await
        .unwrap()
    }

    fn publish(&self, text: &str) {
        self.source.send_modify(|snapshot| {
            let snapshot = snapshot.as_mut().unwrap();
            snapshot.cursor.seq += 1;
            let transcript = Arc::make_mut(&mut snapshot.transcript);
            transcript.entries.push(Entry {
                id: EntryId(11),
                kind: EntryKind::Agent { text: text.into() },
            });
        });
    }

    async fn finish(self) {
        let _ = self.stop.send(true);
        self.serving.await.unwrap().unwrap();
        self.owner.stop_all().await.unwrap();
        self.connection.close(0u32.into(), b"finished");
    }
}

async fn closed(tunnel: &mut io::Tunnel) {
    let result = tokio::time::timeout(Duration::from_secs(2), tunnel.0.io.read_u8())
        .await
        .expect("transport stayed open");
    assert!(result.is_err(), "received data after access ended");
}

#[tokio::test]
async fn native_peer_reads_a_live_filtered_session_through_a_real_relay() {
    let s = Session::new().await;
    let relay = sheprd_relay::Endpoint::parse("https://relay.example/private").unwrap();
    let link = crate::sharing::link::encode(&relay, &s.invite, &s.offer).unwrap();
    let (invite, offer) = crate::sharing::link::decode(link.expose_secret(), &relay, 0).unwrap();
    let mut peer = Peer::connect(
        &s.connection,
        &offer,
        invite.share,
        invite.grant,
        &invite.capability,
    )
    .await
    .unwrap();
    let cursor = match peer.read(None).await.unwrap() {
        Update::Snapshot { cursor, transcript } => {
            assert!(transcript.entries.is_empty());
            assert!(transcript.read_only);
            assert!(transcript.session.is_none());
            cursor
        }
        other => panic!("{other:?}"),
    };
    s.publish("public new reply");
    let update = peer.read(Some(cursor)).await.unwrap();
    assert!(matches!(update, Update::Changes { .. }));
    let bytes = serde_json::to_string(&update).unwrap();
    assert!(bytes.contains("public new reply"));
    assert!(!bytes.contains("private"));
    s.owner
        .revoke(s.invite.share, s.invite.grant)
        .await
        .unwrap();
    closed(&mut peer.tunnel).await;
    s.finish().await;
}

#[tokio::test]
async fn bootstrap_credential_without_the_application_capability_cannot_read() {
    let s = Session::new().await;
    let wrong = Capability::from_bytes([0; 32]);
    assert!(
        Peer::connect(
            &s.connection,
            &s.offer,
            s.invite.share,
            s.invite.grant,
            &wrong
        )
        .await
        .is_err()
    );
    let mut peer = s.peer().await;
    assert!(peer.read(None).await.is_ok());
    s.finish().await;
}

#[tokio::test]
async fn replayed_enrollment_proof_fails_on_another_noise_handshake() {
    let s = Session::new().await;
    let keys = Keys::generate().unwrap();
    let viewer = signing_key().unwrap();
    let bootstrap = p256::ecdsa::SigningKey::from_slice(s.offer.bootstrap.expose_secret()).unwrap();
    let first = tunnel(&s.connection, &s.offer, &keys, &bootstrap)
        .await
        .unwrap();
    let replay = proof::join(
        s.invite.share,
        s.invite.grant,
        &s.invite.capability,
        &viewer,
        &first.0.hash,
    )
    .unwrap();
    let mut second = tunnel(&s.connection, &s.offer, &keys, &bootstrap)
        .await
        .unwrap();
    assert_ne!(first.0.hash, second.0.hash);
    io::write(&mut second.0.io, &replay, MAX_REQUEST_BYTES)
        .await
        .unwrap();
    closed(&mut second).await;
    drop(first);
    s.finish().await;
}

async fn enroll(s: &Session, keys: &Keys, viewer: &p256::ecdsa::SigningKey) {
    let bootstrap = p256::ecdsa::SigningKey::from_slice(s.offer.bootstrap.expose_secret()).unwrap();
    let mut initial = tunnel(&s.connection, &s.offer, keys, &bootstrap)
        .await
        .unwrap();
    let bytes = proof::join(
        s.invite.share,
        s.invite.grant,
        &s.invite.capability,
        viewer,
        &initial.0.hash,
    )
    .unwrap();
    io::write(&mut initial.0.io, &bytes, MAX_REQUEST_BYTES)
        .await
        .unwrap();
    let ack = io::read(&mut initial.0.io, 128).await.unwrap();
    assert_eq!(ack, br#"{"type":"registered","version":0}"#);
}

#[tokio::test]
async fn a_registered_dial_key_cannot_substitute_a_different_tunnel_identity() {
    let s = Session::new().await;
    let viewer = signing_key().unwrap();
    enroll(&s, &Keys::generate().unwrap(), &viewer).await;
    let mut wrong = tunnel(&s.connection, &s.offer, &Keys::generate().unwrap(), &viewer)
        .await
        .unwrap();
    let bytes = proof::resume(s.invite.share, s.invite.grant, &viewer, &wrong.0.hash).unwrap();
    io::write(&mut wrong.0.io, &bytes, MAX_REQUEST_BYTES)
        .await
        .unwrap();
    closed(&mut wrong).await;
    s.finish().await;
}

#[tokio::test]
async fn resume_proof_is_fresh_and_failed_proofs_do_not_consume_the_registration() {
    let s = Session::new().await;
    let keys = Keys::generate().unwrap();
    let viewer = signing_key().unwrap();
    enroll(&s, &keys, &viewer).await;
    let first = tunnel(&s.connection, &s.offer, &keys, &viewer)
        .await
        .unwrap();
    let replay = proof::resume(s.invite.share, s.invite.grant, &viewer, &first.0.hash).unwrap();
    let mut second = tunnel(&s.connection, &s.offer, &keys, &viewer)
        .await
        .unwrap();
    io::write(&mut second.0.io, &replay, MAX_REQUEST_BYTES)
        .await
        .unwrap();
    closed(&mut second).await;
    drop(first);
    let mut third = tunnel(&s.connection, &s.offer, &keys, &viewer)
        .await
        .unwrap();
    let fresh = proof::resume(s.invite.share, s.invite.grant, &viewer, &third.0.hash).unwrap();
    io::write(&mut third.0.io, &fresh, MAX_REQUEST_BYTES)
        .await
        .unwrap();
    assert_eq!(
        io::read(&mut third.0.io, 128).await.unwrap(),
        br#"{"type":"ready","version":0}"#
    );
    s.finish().await;
}

#[tokio::test]
async fn owner_commands_and_oversized_frames_close_only_the_offending_guest() {
    let s = Session::new().await;
    let mut bad = s.peer().await;
    let mut good = s.peer().await;
    io::write(
        &mut bad.tunnel.0.io,
        br#"{"version":0,"cursor":null,"command":"set_auto_accept"}"#,
        MAX_REQUEST_BYTES,
    )
    .await
    .unwrap();
    closed(&mut bad.tunnel).await;
    assert!(good.read(None).await.is_ok());
    good.tunnel.0.io.write_u32(u32::MAX).await.unwrap();
    closed(&mut good.tunnel).await;
    s.finish().await;
}

#[tokio::test]
async fn revocation_cancels_a_partial_read_and_a_source_replacement_closes_idle_streams() {
    let s = Session::new().await;
    let mut peer = s.peer().await;
    peer.tunnel.0.io.write_u32(100).await.unwrap();
    peer.tunnel.0.io.write_u8(b'{').await.unwrap();
    s.owner
        .revoke(s.invite.share, s.invite.grant)
        .await
        .unwrap();
    closed(&mut peer.tunnel).await;
    let invite = s.owner.invite(s.invite.share).await.unwrap();
    let updated_offer = s.route.offer(invite.grant).await.unwrap();
    let mut other = Peer::connect(
        &s.connection,
        &updated_offer,
        invite.share,
        invite.grant,
        &invite.capability,
    )
    .await
    .unwrap();
    s.source
        .send_modify(|source| source.as_mut().unwrap().cursor = Cursor::new_epoch());
    closed(&mut other.tunnel).await;
    assert!(s.owner.invite(invite.share).await.is_err());
    s.finish().await;
}

#[tokio::test]
async fn stopping_the_route_cancels_all_tunnel_pumps() {
    let s = Session::new().await;
    let mut peer = s.peer().await;
    s.finish().await;
    closed(&mut peer.tunnel).await;
}

#[tokio::test]
async fn private_relay_listener_token_is_required_but_never_needed_by_the_guest() {
    let s = Session::with(Some("fixture-private-relay-token"), Duration::from_secs(60)).await;
    assert!(
        Arc::new(Route::new().unwrap())
            .listen(
                s.connection.clone(),
                b"wrong",
                s.owner.access(s.invite.share)
            )
            .await
            .is_err()
    );
    let mut peer = s.peer().await;
    assert!(peer.read(None).await.is_ok());
    s.finish().await;
}

#[tokio::test]
async fn a_wrong_pinned_host_identity_cannot_finish_admission() {
    let s = Session::new().await;
    let wrong = Offer {
        route: s.offer.route,
        identity: Keys::generate().unwrap().identity(),
        bootstrap: secrecy::SecretBox::new(Box::new(*s.offer.bootstrap.expose_secret())),
    };
    assert!(
        Peer::connect(
            &s.connection,
            &wrong,
            s.invite.share,
            s.invite.grant,
            &s.invite.capability
        )
        .await
        .is_err()
    );
    s.finish().await;
}

#[tokio::test]
async fn expiration_closes_an_idle_connection_without_another_read_request() {
    let s = Session::with(None, Duration::from_secs(3)).await;
    let mut peer = s.peer().await;
    let result = tokio::time::timeout(Duration::from_secs(5), peer.tunnel.0.io.read_u8())
        .await
        .unwrap();
    assert!(result.is_err());
    s.finish().await;
}

#[tokio::test]
async fn cancelled_leases_release_the_bounded_worker_viewer_slots() {
    let s = Session::new().await;
    let access = s.owner.access(s.invite.share);
    let mut leases = Vec::new();
    for _ in 0..crate::sharing::MAX_VIEWERS {
        leases.push(
            access
                .join(
                    s.invite.grant,
                    Capability::from_bytes(*s.invite.capability.expose()),
                )
                .await
                .unwrap(),
        );
    }
    assert!(matches!(
        access
            .join(
                s.invite.grant,
                Capability::from_bytes(*s.invite.capability.expose())
            )
            .await,
        Err(crate::sharing::Error::Limit)
    ));
    let lease = leases.pop().unwrap();
    let mut allowed = lease.allowed.clone();
    drop(lease);
    tokio::time::timeout(Duration::from_secs(2), async {
        let _ = allowed.wait_for(|v| !*v).await;
    })
    .await
    .unwrap();
    assert!(
        access
            .join(
                s.invite.grant,
                Capability::from_bytes(*s.invite.capability.expose())
            )
            .await
            .is_ok()
    );
    drop(leases);
    s.finish().await;
}

#[tokio::test]
async fn cancelling_a_reference_peer_read_poisoned_the_connection_instead_of_replaying() {
    let s = Session::new().await;
    let mut peer = s.peer().await;
    assert!(
        futures_lite::future::poll_once(peer.read(None))
            .await
            .is_none()
    );
    assert!(matches!(
        peer.read(None).await,
        Err(crate::sharing::Error::Denied)
    ));
    s.finish().await;
}

#[tokio::test]
async fn bootstrap_credentials_are_grant_scoped_and_removed_on_revocation() {
    let s = Session::new().await;
    let other = s.owner.invite(s.invite.share).await.unwrap();
    let offer = s.route.offer(other.grant).await.unwrap();
    assert_ne!(
        offer.bootstrap.expose_secret(),
        s.offer.bootstrap.expose_secret()
    );
    // Both capabilities are valid, but a transport credential cannot redeem
    // another grant on the same share.
    assert!(
        Peer::connect(
            &s.connection,
            &s.offer,
            other.share,
            other.grant,
            &other.capability
        )
        .await
        .is_err()
    );
    let mut peer = Peer::connect(
        &s.connection,
        &offer,
        other.share,
        other.grant,
        &other.capability,
    )
    .await
    .unwrap();
    s.owner.revoke(other.share, other.grant).await.unwrap();
    closed(&mut peer.tunnel).await;
    let dial = p256::ecdsa::SigningKey::from_slice(offer.bootstrap.expose_secret()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match client::dial(&s.connection, offer.route, &dial).await {
                Err(client::Error::Refused(sheprd_relay::wire::Status::Offline)) => {
                    break;
                }
                Ok(stream) => drop(stream),
                other => panic!("unexpected relay answer: {other:?}"),
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    s.finish().await;
}
