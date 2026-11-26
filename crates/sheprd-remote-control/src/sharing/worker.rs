//! One bounded serial worker owns persistence and guest projections. The source
//! is a coalesced watch, not a queue of complete transcripts. No disk I/O or
//! projection work runs on the async reactor or the UI thread.

use crate::sharing::{
    Capability, Clock, Content, Error, GrantId, Guest, History, Invitation, Result, ShareId,
    Source, Store,
};
use sheprd_chat_protocol::{Cursor, EndpointId, Transcript, sharing::Update};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{oneshot, watch};
use uuid::Uuid;

const QUEUE: usize = 32;
const CHECK_EVERY: Duration = Duration::from_millis(100);

/// One host-observed source state. `None` in its watch means disconnected.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub endpoint: EndpointId,
    pub cursor: Cursor,
    pub transcript: Arc<Transcript>,
}

impl Snapshot {
    fn source(&self) -> Source<'_> {
        Source {
            endpoint: &self.endpoint,
            cursor: self.cursor,
            transcript: &self.transcript,
        }
    }
}

pub type Feed = watch::Receiver<Option<Snapshot>>;

/// Local owner authority. Never pass this handle to a guest session dispatcher.
#[derive(Clone, Debug)]
pub struct Owner {
    sender: SyncSender<Message>,
}

/// Only one share's admission and read operations, with no owner authority.
#[derive(Clone, Debug)]
pub struct Access {
    sender: SyncSender<Message>,
    share: ShareId,
}

/// Dropping a stream releases its slot, even if its async task was cancelled.
#[derive(Debug)]
pub(crate) struct Lease {
    id: Uuid,
    cancelled: Arc<AtomicBool>,
    pub(crate) allowed: watch::Receiver<bool>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Lease {
    pub(crate) fn cancelled(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

type Reply<T> = oneshot::Sender<Result<T>>;

enum Message {
    // keep-sorted start
    Create {
        source: Feed,
        history: History,
        content: Content,
        lifetime: Duration,
        reply: Reply<ShareId>,
    },
    Grants {
        share: ShareId,
        reply: Reply<Vec<GrantId>>,
    },
    Invite {
        share: ShareId,
        reply: Reply<Invitation>,
    },
    Join {
        share: ShareId,
        grant: GrantId,
        capability: Capability,
        reply: Reply<Lease>,
    },
    Read {
        share: ShareId,
        lease: Uuid,
        request: Vec<u8>,
        reply: Reply<Update>,
    },
    Revoke {
        share: ShareId,
        grant: GrantId,
        reply: Reply<()>,
    },
    Stop {
        share: ShareId,
        reply: Reply<()>,
    },
    StopAll {
        reply: Reply<()>,
    },
    // keep-sorted end
}

struct Reader {
    share: ShareId,
    guest: Guest,
    cancelled: Arc<AtomicBool>,
    allowed: watch::Sender<bool>,
}

struct Worker {
    store: Store,
    sources: BTreeMap<ShareId, Feed>,
    readers: BTreeMap<Uuid, Reader>,
    start: Instant,
    bandwidth_since: Instant,
    bandwidth_bytes: usize,
}

impl Owner {
    /// Blocking initialization; callers on an async/UI thread use their existing
    /// blocking executor. The thread exits after its last command handle drops.
    pub fn open(directory: &Path) -> Result<Self> {
        let start = Instant::now();
        let store = Store::open(directory, now(start)?)?;
        let (sender, receiver) = mpsc::sync_channel(QUEUE);
        thread::Builder::new()
            .name("sheprd-sharing".into())
            .spawn(move || {
                let mut worker = Worker {
                    store,
                    sources: BTreeMap::new(),
                    readers: BTreeMap::new(),
                    start,
                    bandwidth_since: Instant::now(),
                    bandwidth_bytes: 0,
                };
                loop {
                    match receiver.recv_timeout(CHECK_EVERY) {
                        Ok(message) => worker.handle(message),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    worker.sweep();
                }
                // Dropping the senders tells every transport to close.
            })?;
        Ok(Self { sender })
    }

    pub async fn create(
        &self,
        source: Feed,
        history: History,
        content: Content,
        lifetime: Duration,
    ) -> Result<ShareId> {
        call(&self.sender, |reply| Message::Create {
            source,
            history,
            content,
            lifetime,
            reply,
        })
        .await
    }

    pub async fn invite(&self, share: ShareId) -> Result<Invitation> {
        call(&self.sender, |reply| Message::Invite { share, reply }).await
    }

    pub async fn revoke(&self, share: ShareId, grant: GrantId) -> Result<()> {
        call(&self.sender, |reply| Message::Revoke {
            share,
            grant,
            reply,
        })
        .await
    }

    pub async fn stop(&self, share: ShareId) -> Result<()> {
        call(&self.sender, |reply| Message::Stop { share, reply }).await
    }

    pub async fn stop_all(&self) -> Result<()> {
        call(&self.sender, |reply| Message::StopAll { reply }).await
    }

    pub fn access(&self, share: ShareId) -> Access {
        Access {
            sender: self.sender.clone(),
            share,
        }
    }
}

impl Access {
    pub fn share(&self) -> ShareId {
        self.share
    }

    pub(crate) async fn grants(&self) -> Result<Vec<GrantId>> {
        call(&self.sender, |reply| Message::Grants {
            share: self.share,
            reply,
        })
        .await
    }

    pub(crate) async fn join(&self, grant: GrantId, capability: Capability) -> Result<Lease> {
        call(&self.sender, |reply| Message::Join {
            share: self.share,
            grant,
            capability,
            reply,
        })
        .await
    }

    pub(crate) async fn read(&self, lease: &Lease, request: Vec<u8>) -> Result<Update> {
        if request.len() > sheprd_chat_protocol::sharing::MAX_REQUEST_BYTES {
            return Err(Error::Limit);
        }
        call(&self.sender, |reply| Message::Read {
            share: self.share,
            lease: lease.id,
            request,
            reply,
        })
        .await
    }
}

async fn call<T>(
    sender: &SyncSender<Message>,
    message: impl FnOnce(Reply<T>) -> Message,
) -> Result<T> {
    let (reply, answer) = oneshot::channel();
    sender.try_send(message(reply)).map_err(|e| match e {
        mpsc::TrySendError::Full(_) => Error::Limit,
        mpsc::TrySendError::Disconnected(_) => Error::Unavailable,
    })?;
    answer.await.map_err(|_| Error::Unavailable)?
}

fn now(start: Instant) -> Result<Clock> {
    let unix_ms = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Clock)?
            .as_millis(),
    )
    .map_err(|_| Error::Clock)?;
    Ok(Clock {
        unix_ms,
        elapsed: start.elapsed(),
    })
}

fn snapshot(source: &Feed) -> Result<Snapshot> {
    if source.has_changed().is_err() {
        return Err(Error::Source);
    }
    source.borrow().clone().ok_or(Error::Source)
}

impl Worker {
    fn handle(&mut self, message: Message) {
        match message {
            Message::Create {
                source,
                history,
                content,
                lifetime,
                reply,
            } => {
                if reply.is_closed() {
                    return;
                }
                let result = (|| {
                    let snapshot = snapshot(&source)?;
                    let id = self.store.create(
                        snapshot.source(),
                        history,
                        content,
                        lifetime,
                        now(self.start)?,
                    )?;
                    // Store prunes old records when creating; keep feeds bounded too.
                    self.sources.retain(|id, _| self.store.index(*id).is_ok());
                    self.sources.insert(id, source);
                    Ok(id)
                })();
                let _ = reply.send(result);
            }
            Message::Grants { share, reply } => {
                let result = (|| {
                    let now = now(self.start)?;
                    self.store.tick(now)?;
                    Ok(self
                        .store
                        .active(share, now)?
                        .grants
                        .iter()
                        .filter(|g| !g.revoked && g.expires_ms > now.unix_ms)
                        .map(|g| g.id)
                        .collect())
                })();
                let _ = reply.send(result);
            }
            Message::Invite { share, reply } => {
                if !reply.is_closed() {
                    let _ =
                        reply.send(now(self.start).and_then(|now| self.store.invite(share, now)));
                }
            }
            Message::Revoke {
                share,
                grant,
                reply,
            } => {
                let result = now(self.start).and_then(|now| self.store.revoke(share, grant, now));
                self.sweep();
                let _ = reply.send(result);
            }
            Message::Stop { share, reply } => {
                let result = now(self.start).and_then(|now| self.store.stop(share, now));
                self.sweep();
                let _ = reply.send(result);
            }
            Message::StopAll { reply } => {
                let result = now(self.start).and_then(|now| self.store.stop_all(now));
                self.sweep();
                let _ = reply.send(result);
            }
            Message::Join {
                share,
                grant,
                capability,
                reply,
            } => {
                if reply.is_closed() {
                    return;
                }
                let result = (|| {
                    let source = snapshot(self.sources.get(&share).ok_or(Error::Denied)?)?;
                    let guest = self.store.admit(
                        share,
                        grant,
                        &capability,
                        source.source(),
                        now(self.start)?,
                    )?;
                    let id = Uuid::new_v4();
                    let cancelled = Arc::new(AtomicBool::new(false));
                    let (allowed, changes) = watch::channel(true);
                    self.readers.insert(
                        id,
                        Reader {
                            share,
                            guest,
                            cancelled: Arc::clone(&cancelled),
                            allowed,
                        },
                    );
                    Ok(Lease {
                        id,
                        cancelled,
                        allowed: changes,
                    })
                })();
                // A cancelled receiver drops its lease here, releasing admission.
                let _ = reply.send(result);
            }
            Message::Read {
                share,
                lease,
                request,
                reply,
            } => {
                if reply.is_closed() {
                    return;
                }
                let result = (|| {
                    let reader = self
                        .readers
                        .get(&lease)
                        .filter(|r| r.share == share && !r.cancelled.load(Ordering::Acquire))
                        .ok_or(Error::Denied)?;
                    let source = snapshot(self.sources.get(&share).ok_or(Error::Denied)?)?;
                    let update = self.store.read(
                        &reader.guest,
                        &request,
                        source.source(),
                        now(self.start)?,
                    )?;
                    if self.bandwidth_since.elapsed() >= Duration::from_secs(1) {
                        self.bandwidth_since = Instant::now();
                        self.bandwidth_bytes = 0;
                    }
                    let bytes = serde_json::to_vec(&update)?.len();
                    if self.bandwidth_bytes + bytes > 8 * 1024 * 1024 {
                        return Err(Error::Limit);
                    }
                    self.bandwidth_bytes += bytes;
                    Ok(update)
                })();
                let _ = reply.send(result);
            }
        }
    }

    fn sweep(&mut self) {
        let expired: Vec<_> = self
            .readers
            .iter()
            .filter_map(|(id, reader)| {
                let allowed = !reader.cancelled.load(Ordering::Acquire)
                    && (|| {
                        let source =
                            snapshot(self.sources.get(&reader.share).ok_or(Error::Denied)?)?;
                        self.store
                            .check(&reader.guest, source.source(), now(self.start)?)
                    })()
                    .is_ok();
                (!allowed).then_some(*id)
            })
            .collect();
        for id in expired {
            if let Some(reader) = self.readers.remove(&id) {
                let _ = reader.allowed.send(false);
                self.store.disconnect(reader.guest);
            }
        }
    }
}
