//! A dedicated per-share relay route. Bootstrap dials enroll a proven per-viewer
//! key, then close without a transcript. Viewers reconnect on their own key and
//! prove it over the new Noise handshake. No HTTP router or fleet dispatch is
//! reachable from this channel.

pub mod io;
pub mod peer;
pub mod proof;
mod session;

use crate::sharing::{
    Error, GrantId, MAX_VIEWERS, Result,
    worker::{Access, Lease},
};
use p256::ecdsa::SigningKey;
use secrecy::{ExposeSecret, SecretBox};
use sheprd_relay::{
    RouteId,
    client::{self, Listener},
    wire::Key,
};
use sheprd_tunnel::{Identity, Keys};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{Semaphore, mpsc, oneshot, watch},
    task::JoinSet,
};

const ADMISSION: Duration = Duration::from_secs(5);
const SWEEP: Duration = Duration::from_millis(100);
const MAX_PENDING: usize = 4;
const MAX_TASKS: usize = MAX_VIEWERS + MAX_PENDING;

/// Public host identity and a relay-only bootstrap credential. The latter is
/// deliberately independent of the application grant capability.
#[derive(Debug)]
pub struct Offer {
    pub route: RouteId,
    pub identity: Identity,
    pub bootstrap: SecretBox<[u8; 32]>,
}

#[derive(Debug)]
pub struct Route {
    route: SigningKey,
    seed: SecretBox<[u8; 32]>,
    ready: watch::Sender<Vec<GrantId>>,
    tunnel: Keys,
}

impl Route {
    pub fn new() -> Result<Self> {
        Ok(Self {
            route: signing_key()?,
            seed: SecretBox::new(Box::new(
                crate::sharing::Capability::generate()?.expose().to_owned(),
            )),
            ready: watch::Sender::new(Vec::new()),
            tunnel: Keys::generate()?,
        })
    }

    /// Return an offer only after this grant's bootstrap key is installed.
    pub async fn offer(&self, grant: GrantId) -> Result<Offer> {
        let mut ready = self.ready.subscribe();
        tokio::time::timeout(ADMISSION, async {
            let _ = ready
                .wait_for(|grants| grants.contains(&grant))
                .await
                .map_err(|_| Error::Unavailable)?;
            Ok::<_, Error>(())
        })
        .await
        .map_err(|_| Error::Timeout)??;
        Ok(Offer {
            route: client::route(&self.route),
            identity: self.tunnel.identity(),
            bootstrap: SecretBox::new(Box::new(self.bootstrap(grant).to_bytes().into())),
        })
    }

    fn bootstrap(&self, grant: GrantId) -> SigningKey {
        let mut counter = 0u64;
        loop {
            let bytes = crate::auth::sha256(&[
                b"sheprd-share-bootstrap/v0\0",
                self.seed.expose_secret(),
                grant.0.as_bytes(),
                &counter.to_be_bytes(),
            ]);
            if let Ok(key) = SigningKey::from_slice(&bytes) {
                return key;
            }
            counter = counter.wrapping_add(1);
        }
    }

    fn bootstraps(&self, grants: &[GrantId]) -> BTreeMap<Key, GrantId> {
        grants
            .iter()
            .map(|grant| (client::public(&self.bootstrap(*grant)), *grant))
            .collect()
    }

    /// The caller chooses an already trusted relay connection. No URL from an
    /// invitation is fetched by the host. Private relay listen tokens stay here.
    pub async fn listen(
        self: Arc<Self>,
        connection: quinn::Connection,
        token: &[u8],
        access: Access,
    ) -> Result<Serving> {
        tokio::time::timeout(ADMISSION, async {
            let mut listener = Listener::listen(connection, &self.route, token).await?;
            let grants = access.grants().await?;
            let bootstraps = Arc::new(self.bootstraps(&grants));
            listener
                .set_dial_keys(bootstraps.keys().copied().collect())
                .await?;
            self.ready.send_replace(grants);
            Ok(Serving {
                listener,
                route: self,
                access,
                bootstraps,
            })
        })
        .await
        .map_err(|_| Error::Timeout)?
    }
}

#[derive(Debug)]
pub struct Serving {
    bootstraps: Arc<BTreeMap<Key, GrantId>>,
    listener: Listener,
    route: Arc<Route>,
    access: Access,
}

struct Registration {
    grant: GrantId,
    remote: [u8; 32],
    pending: Option<Lease>,
    cancelled: Arc<AtomicBool>,
    allowed: watch::Receiver<bool>,
    since: Instant,
}

enum Event {
    // keep-sorted start
    Claim {
        key: Key,
        remote: [u8; 32],
        hash: [u8; 32],
        proof: proof::Resume,
        reply: oneshot::Sender<Result<Lease>>,
    },
    Register {
        key: Key,
        grant: GrantId,
        remote: [u8; 32],
        lease: Lease,
        reply: oneshot::Sender<Result<()>>,
    },
    // keep-sorted end
}

impl Serving {
    /// Runs until explicitly stopped, its stop sender disappears, or the relay
    /// fails. Its task set and tunnel pumps are cancelled together on every exit.
    pub async fn run(self, mut stopped: watch::Receiver<bool>) -> Result<()> {
        tokio::select! {
            biased;
            _ = async { let _ = stopped.wait_for(|stop| *stop).await; } => Ok(()),
            result = self.serve() => result,
        }
    }

    async fn serve(mut self) -> Result<()> {
        let (sender, mut events) = mpsc::channel(MAX_TASKS);
        let mut sessions = JoinSet::new();
        let mut registered = BTreeMap::<Key, Registration>::new();
        let pending = Arc::new(Semaphore::new(MAX_PENDING));
        let mut sweep = tokio::time::interval(SWEEP);
        let mut window = Instant::now();
        let mut attempts = 0;
        loop {
            tokio::select! {
                biased;
                _ = sweep.tick() => {
                    let grants = match tokio::time::timeout(ADMISSION, self.access.grants()).await.map_err(|_| Error::Timeout)? {
                        Ok(grants) => grants,
                        Err(Error::Denied | Error::Disabled | Error::Unavailable) => return Ok(()),
                        Err(error) => return Err(error),
                    };
                    let changed = grants.len() != self.bootstraps.len() || self.bootstraps.values().any(|g| !grants.contains(g));
                    if changed { self.bootstraps = Arc::new(self.route.bootstraps(&grants)); }
                    let before = registered.len();
                    registered.retain(|_, entry| grants.contains(&entry.grant) && !entry.cancelled.load(Ordering::Acquire)
                        && *entry.allowed.borrow() && entry.allowed.has_changed().is_ok()
                        && !(entry.pending.is_some() && entry.since.elapsed() >= ADMISSION));
                    if changed || registered.len() != before {
                        update_keys(&mut self.listener, &self.bootstraps, &registered).await?;
                        self.route.ready.send_replace(grants);
                    }
                }
                Some(event) = events.recv() => match event {
                    Event::Register { key, grant, remote, lease, reply } => {
                        if reply.is_closed() { continue; }
                        if self.bootstraps.contains_key(&key) || !self.bootstraps.values().any(|g| *g == grant) || registered.contains_key(&key) || registered.len() >= MAX_VIEWERS || !*lease.allowed.borrow() {
                            let _ = reply.send(Err(Error::Denied));
                            continue;
                        }
                        let cancelled = lease.cancelled();
                        let allowed = lease.allowed.clone();
                        registered.insert(key, Registration { grant, remote, pending: Some(lease), cancelled, allowed, since: Instant::now() });
                        // Acknowledgement means the relay actually installed the key.
                        update_keys(&mut self.listener, &self.bootstraps, &registered).await?;
                        if reply.send(Ok(())).is_err() {
                            registered.remove(&key);
                            update_keys(&mut self.listener, &self.bootstraps, &registered).await?;
                        }
                    },
                    Event::Claim { key, remote, hash, proof, reply } => {
                        if reply.is_closed() { continue; }
                        let result = (|| {
                            let entry = registered.get_mut(&key).ok_or(Error::Denied)?;
                            if entry.remote != remote || !*entry.allowed.borrow() || entry.since.elapsed() >= ADMISSION { return Err(Error::Denied); }
                            proof.verify(&key, self.access.share(), entry.grant, &hash)?;
                            entry.pending.take().ok_or(Error::Denied)
                        })();
                        let _ = reply.send(result);
                    },
                },
                incoming = self.listener.next_stream() => {
                    let incoming = incoming?;
                    if window.elapsed() >= Duration::from_secs(1) { window = Instant::now(); attempts = 0; }
                    // Bound tasks before reading the relay's untrusted header.
                    if sessions.len() >= MAX_TASKS || attempts >= 8 { drop(incoming); continue; }
                    attempts += 1;
                    sessions.spawn(session::serve(incoming, Arc::clone(&self.route), self.access.clone(), sender.clone(), Arc::clone(&pending), Arc::clone(&self.bootstraps)));
                },
                Some(_) = sessions.join_next(), if !sessions.is_empty() => {},
                error = self.listener.closed() => return Err(error.into()),
            }
        }
    }
}

impl Drop for Serving {
    fn drop(&mut self) {
        self.route.ready.send_replace(Vec::new());
    }
}

async fn update_keys(
    listener: &mut Listener,
    bootstraps: &BTreeMap<Key, GrantId>,
    registered: &BTreeMap<Key, Registration>,
) -> Result<()> {
    let keys = bootstraps
        .keys()
        .chain(registered.keys())
        .copied()
        .collect();
    tokio::time::timeout(ADMISSION, listener.set_dial_keys(keys))
        .await
        .map_err(|_| Error::Timeout)??;
    Ok(())
}

fn signing_key() -> Result<SigningKey> {
    loop {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(Error::Random)?;
        if let Ok(key) = SigningKey::from_slice(&bytes) {
            return Ok(key);
        }
    }
}
