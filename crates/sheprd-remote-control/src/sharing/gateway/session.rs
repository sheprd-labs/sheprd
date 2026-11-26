use crate::sharing::{
    Error, Result,
    gateway::{
        ADMISSION, Event, Route,
        io::{self, Tunnel},
        proof,
    },
    worker::{Access, Lease},
};
use sheprd_chat_protocol::sharing::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES};
use sheprd_relay::client;
use sheprd_tunnel::{Responder, stream};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::{
    io::join,
    sync::{Semaphore, mpsc, oneshot},
};

pub(super) async fn serve(
    pending: client::Pending,
    route: Arc<Route>,
    access: Access,
    events: mpsc::Sender<Event>,
    slots: Arc<Semaphore>,
    bootstraps: Arc<BTreeMap<sheprd_relay::wire::Key, crate::sharing::GrantId>>,
) {
    // No peer data or per-session identifiers go into logs.
    let _ = run(pending, route, access, events, slots, bootstraps).await;
}

async fn run(
    pending: client::Pending,
    route: Arc<Route>,
    access: Access,
    events: mpsc::Sender<Event>,
    slots: Arc<Semaphore>,
    bootstraps: Arc<BTreeMap<sheprd_relay::wire::Key, crate::sharing::GrantId>>,
) -> Result<()> {
    // All admission work, including an untrusted relay header, shares the
    // same slot and deadline. Registered keys do not bypass this budget.
    let permit = slots.try_acquire_owned().map_err(|_| Error::Limit)?;
    let deadline = tokio::time::Instant::now() + ADMISSION;
    let incoming = tokio::time::timeout_at(deadline, pending.incoming())
        .await
        .map_err(|_| Error::Timeout)??;
    let key = incoming.key;
    let bootstrap = bootstraps.get(&key).copied();
    let id = client::route(&route.route);
    let admitted = tokio::time::timeout_at(deadline, async {
        // This route has no data before the application proof. Accepting a Noise
        // initiator here grants only the chance to prove the capability/key.
        let responder = Responder::session(&route.tunnel, id.as_bytes())?;
        let mut tunnel = Tunnel(
            stream::accept(
                join(incoming.recv, incoming.send),
                &route.tunnel,
                responder,
                |_| true,
            )
            .await?,
        );
        let bytes = io::read(&mut tunnel.0.io, MAX_REQUEST_BYTES).await?;
        if let Some(grant) = bootstrap {
            let claim: proof::Join = serde_json::from_slice(&bytes)?;
            if claim.grant != grant {
                return Err(Error::Denied);
            }
            let capability = claim.verify(access.share(), &tunnel.0.hash)?;
            let lease = access.join(claim.grant, capability).await?;
            let key = claim.key.as_slice().try_into().map_err(|_| Error::Denied)?;
            let (reply, answer) = oneshot::channel();
            events
                .try_send(Event::Register {
                    key,
                    grant: claim.grant,
                    remote: tunnel.0.remote,
                    lease,
                    reply,
                })
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => Error::Limit,
                    mpsc::error::TrySendError::Closed(_) => Error::Unavailable,
                })?;
            answer.await.map_err(|_| Error::Unavailable)??;
            io::write(
                &mut tunnel.0.io,
                br#"{"type":"registered","version":0}"#,
                MAX_RESPONSE_BYTES,
            )
            .await?;
            // Wait for the client to read the acknowledgement and close. Dropping
            // the pump immediately could otherwise discard buffered plaintext.
            let mut end = [0; 1];
            let _ = tokio::io::AsyncReadExt::read(&mut tunnel.0.io, &mut end).await;
            Ok((tunnel, None))
        } else {
            let proof: proof::Resume = serde_json::from_slice(&bytes)?;
            let (reply, answer) = oneshot::channel();
            events
                .try_send(Event::Claim {
                    key,
                    remote: tunnel.0.remote,
                    hash: tunnel.0.hash,
                    proof,
                    reply,
                })
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => Error::Limit,
                    mpsc::error::TrySendError::Closed(_) => Error::Unavailable,
                })?;
            let lease = answer.await.map_err(|_| Error::Unavailable)??;
            io::write(
                &mut tunnel.0.io,
                br#"{"type":"ready","version":0}"#,
                MAX_RESPONSE_BYTES,
            )
            .await?;
            Ok::<_, Error>((tunnel, Some(lease)))
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    drop(permit);
    let (mut tunnel, lease) = admitted;
    if let Some(mut lease) = lease {
        let mut allowed = lease.allowed.clone();
        tokio::select! {
            biased;
            _ = async { let _ = allowed.wait_for(|allowed| !*allowed).await; } => {},
            result = reads(&access, &mut lease, &mut tunnel) => result?,
        }
    }
    Ok(())
}

async fn reads(access: &Access, lease: &mut Lease, tunnel: &mut Tunnel) -> Result<()> {
    loop {
        if !*lease.allowed.borrow() || lease.allowed.has_changed().is_err() {
            return Err(Error::Denied);
        }
        let request = tokio::time::timeout(
            Duration::from_secs(30),
            io::read(&mut tunnel.0.io, MAX_REQUEST_BYTES),
        )
        .await
        .map_err(|_| Error::Timeout)??;
        let update = access.read(lease, request).await?;
        let bytes = serde_json::to_vec(&update)?;
        if !*lease.allowed.borrow() {
            return Err(Error::Denied);
        }
        tokio::time::timeout(
            ADMISSION,
            io::write(&mut tunnel.0.io, &bytes, MAX_RESPONSE_BYTES),
        )
        .await
        .map_err(|_| Error::Timeout)??;
        // At most four full responses per second, no queued broadcasts.
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
