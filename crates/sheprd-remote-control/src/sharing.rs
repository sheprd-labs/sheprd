//! Host-side read-only sharing foundation, not yet wired to a network listener.
//!
//! The owner calls management methods on the daemon worker. A guest has only
//! `admit`, `read`, and `disconnect`; no local attach command is accepted. Every
//! read rechecks the grant and source before projecting. Relay admission and
//! fleet pairing are deliberately outside this module.

mod error;
pub mod gateway;
pub mod link;
mod model;
mod projection;
pub(crate) mod service;
mod storage;
pub mod worker;

pub use error::{Error, Result};
pub use model::{
    Capability, Clock, Content, GrantId, History, Invitation, MAX_GRANTS, MAX_LIFETIME, MAX_SHARES,
    MAX_VIEWERS, ShareId, Source,
};

use std::{collections::BTreeMap, path::Path, time::Duration};

use sheprd_chat_protocol::sharing::{MAX_REQUEST_BYTES, Read, Update, VERSION};
use uuid::Uuid;

use crate::sharing::{
    model::{Binding, Grant, Share},
    projection::Projection,
    storage::{State, Storage},
};

/// An admitted connection. Not serializable, cloneable, or constructible by a
/// recipient. The gateway must disconnect it when its transport closes.
#[derive(Debug)]
pub struct Guest(Uuid);

#[derive(Debug)]
struct Reader {
    grant: GrantId,
    projection: Projection,
    share: ShareId,
}

/// Owned by one serial host worker. Disk operations are blocking. Opening the
/// store closes old grants durably: resuming sharing across daemon restarts
/// requires a separately tested source-resumption protocol.
#[derive(Debug)]
pub struct Store {
    clock: Clock,
    deadlines: BTreeMap<ShareId, Duration>,
    disabled: bool,
    readers: BTreeMap<Uuid, Reader>,
    state: State,
    storage: Storage,
}

impl Store {
    pub fn open(directory: &Path, now: Clock) -> Result<Self> {
        let (storage, mut state) = Storage::open(directory)?;
        for share in &mut state.shares {
            share.stopped = true;
        }
        storage.save(&state)?;
        Ok(Self {
            clock: now,
            deadlines: BTreeMap::new(),
            disabled: false,
            readers: BTreeMap::new(),
            state,
            storage,
        })
    }

    /// Owner-only: persist the exact history boundary and source observed when
    /// the person confirms sharing. There is no grant until `invite` succeeds.
    pub fn create(
        &mut self,
        source: Source<'_>,
        history: History,
        content: Content,
        lifetime: Duration,
        now: Clock,
    ) -> Result<ShareId> {
        self.tick(now)?;
        projection::validate(source.transcript)?;
        if history == History::FromNow
            && source.transcript.truncated
            && source.transcript.entries.is_empty()
        {
            // A fully truncated snapshot has lost its entry high-water mark.
            // Guessing zero would admit older entries after a reconnect.
            return Err(Error::Source);
        }
        let binding = Binding::capture(source)?;
        if lifetime.is_zero() || lifetime > MAX_LIFETIME || lifetime.as_millis() == 0 {
            return Err(Error::Limit);
        }
        let expires_ms = now
            .unix_ms
            .checked_add(lifetime.as_millis() as u64)
            .ok_or(Error::Clock)?;
        let deadline = now.elapsed.checked_add(lifetime).ok_or(Error::Clock)?;
        self.state.shares.retain(|s| {
            !s.stopped
                && s.expires_ms > now.unix_ms
                && self
                    .deadlines
                    .get(&s.id)
                    .is_some_and(|deadline| *deadline > now.elapsed)
        });
        self.deadlines
            .retain(|id, _| self.state.shares.iter().any(|s| s.id == *id));
        self.readers
            .retain(|_, r| self.state.shares.iter().any(|s| s.id == r.share));
        if self.state.shares.len() >= MAX_SHARES {
            return Err(Error::Limit);
        }
        let id = ShareId(Uuid::new_v4());
        self.state.shares.push(Share {
            id,
            binding,
            boundary: match history {
                History::All => None,
                History::FromNow => source.transcript.last_entry_id(),
            },
            content,
            created_ms: now.unix_ms,
            expires_ms,
            grants: Vec::new(),
            source_seq: source.cursor.seq,
            stopped: false,
        });
        self.deadlines.insert(id, deadline);
        self.save()?;
        Ok(id)
    }

    /// Owner-only: a separate random capability for each revocable view grant.
    /// The secret is returned once; only its domain-separated digest is saved.
    pub fn invite(&mut self, share: ShareId, now: Clock) -> Result<Invitation> {
        self.tick(now)?;
        self.active(share, now)?;
        let index = self.index(share)?;
        if self.state.shares[index].grants.len() >= MAX_GRANTS {
            return Err(Error::Limit);
        }
        let capability = Capability::generate()?;
        let id = GrantId(Uuid::new_v4());
        let expires_ms = self.state.shares[index].expires_ms;
        self.state.shares[index].grants.push(Grant {
            id,
            digest: capability.digest(),
            expires_ms,
            revoked: false,
        });
        self.save()?;
        Ok(Invitation {
            share,
            grant: id,
            capability,
            expires_ms,
        })
    }

    /// Called only inside a host-authenticated end-to-end transport. A
    /// successful relay dial is not a capability. Never accept these bytes via
    /// a GET, a URL query, or the paired-phone API.
    pub fn admit(
        &mut self,
        share: ShareId,
        grant: GrantId,
        capability: &Capability,
        source: Source<'_>,
        now: Clock,
    ) -> Result<Guest> {
        self.tick(now)?;
        self.source(share, source, now)?;
        let record = self.active(share, now)?;
        let allowed = record.grants.iter().any(|g| {
            g.id == grant
                && !g.revoked
                && g.expires_ms > now.unix_ms
                && crate::auth::constant_time_eq(&g.digest, &capability.digest())
        });
        if !allowed {
            return Err(Error::Denied);
        }
        self.prune(now);
        if self.readers.values().filter(|r| r.share == share).count() >= MAX_VIEWERS {
            return Err(Error::Limit);
        }
        let id = Uuid::new_v4();
        self.readers.insert(
            id,
            Reader {
                grant,
                projection: Projection::new(),
                share,
            },
        );
        Ok(Guest(id))
    }

    /// The entire guest request surface. Unknown fields, attach commands and
    /// unsupported versions fail closed. A stale cursor gets a filtered snapshot.
    pub fn read(
        &mut self,
        guest: &Guest,
        request: &[u8],
        source: Source<'_>,
        now: Clock,
    ) -> Result<Update> {
        self.check(guest, source, now)?;
        let reader = self.readers.get(&guest.0).ok_or(Error::Denied)?;
        let record = self.active(reader.share, now)?;
        if request.len() > MAX_REQUEST_BYTES {
            return Err(Error::Limit);
        }
        let request: Read = serde_json::from_slice(request)?;
        if request.version != VERSION {
            return Err(Error::Version);
        }
        let (boundary, content) = (record.boundary, record.content);
        let reader = self.readers.get_mut(&guest.0).ok_or(Error::Denied)?;
        let update =
            reader
                .projection
                .read(source.transcript, boundary, content, request.cursor)?;
        Ok(update)
    }

    /// Revalidate an admitted stream even when it is idle or its output stalls.
    pub fn check(&mut self, guest: &Guest, source: Source<'_>, now: Clock) -> Result<()> {
        self.tick(now)?;
        self.prune(now);
        let reader = self.readers.get(&guest.0).ok_or(Error::Denied)?;
        let share = reader.share;
        let grant = reader.grant;
        self.source(share, source, now)?;
        let record = self.active(share, now)?;
        if !record
            .grants
            .iter()
            .any(|g| g.id == grant && !g.revoked && g.expires_ms > now.unix_ms)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }

    pub fn disconnect(&mut self, guest: Guest) {
        self.readers.remove(&guest.0);
    }

    /// Owner-only preview through the same projection used by admitted readers.
    pub fn preview(&mut self, share: ShareId, source: Source<'_>, now: Clock) -> Result<Update> {
        self.tick(now)?;
        self.source(share, source, now)?;
        let record = self.active(share, now)?;
        Projection::new().read(source.transcript, record.boundary, record.content, None)
    }

    pub fn revoke(&mut self, share: ShareId, grant: GrantId, now: Clock) -> Result<()> {
        self.tick(now)?;
        let index = self.index(share)?;
        let record = self.state.shares[index]
            .grants
            .iter_mut()
            .find(|g| g.id == grant)
            .ok_or(Error::Denied)?;
        record.revoked = true;
        self.readers
            .retain(|_, r| r.share != share || r.grant != grant);
        self.save()
    }

    pub fn stop(&mut self, share: ShareId, now: Clock) -> Result<()> {
        self.tick(now)?;
        let index = self.index(share)?;
        self.state.shares[index].stopped = true;
        self.deadlines.remove(&share);
        self.readers.retain(|_, r| r.share != share);
        self.save()
    }

    /// Owner kill switch. It does not stop agents or paired-device connections.
    pub fn stop_all(&mut self, now: Clock) -> Result<()> {
        self.tick(now)?;
        for share in &mut self.state.shares {
            share.stopped = true;
        }
        self.readers.clear();
        self.deadlines.clear();
        self.save()
    }

    fn tick(&mut self, now: Clock) -> Result<()> {
        if self.disabled {
            return Err(Error::Disabled);
        }
        if now.unix_ms < self.clock.unix_ms || now.elapsed < self.clock.elapsed {
            self.disabled = true;
            self.readers.clear();
            return Err(Error::Clock);
        }
        self.clock = now;
        Ok(())
    }

    fn index(&self, id: ShareId) -> Result<usize> {
        self.state
            .shares
            .iter()
            .position(|s| s.id == id)
            .ok_or(Error::Denied)
    }

    fn active(&self, id: ShareId, now: Clock) -> Result<&Share> {
        let share = &self.state.shares[self.index(id)?];
        if self.disabled
            || share.stopped
            || share.expires_ms <= now.unix_ms
            || !self
                .deadlines
                .get(&id)
                .is_some_and(|deadline| *deadline > now.elapsed)
        {
            return Err(Error::Denied);
        }
        Ok(share)
    }

    fn source(&mut self, id: ShareId, source: Source<'_>, now: Clock) -> Result<()> {
        let index = self.index(id)?;
        let record = self.active(id, now)?;
        if !record.binding.matches(source) || source.cursor.seq < record.source_seq {
            self.stop(id, now)?;
            return Err(Error::Source);
        }
        projection::validate(source.transcript)?;
        let share = &mut self.state.shares[index];
        share.source_seq = share.source_seq.max(source.cursor.seq);
        Ok(())
    }

    fn prune(&mut self, now: Clock) {
        self.readers.retain(|_, r| {
            self.state.shares.iter().any(|s| {
                s.id == r.share
                    && !s.stopped
                    && s.expires_ms > now.unix_ms
                    && self
                        .deadlines
                        .get(&s.id)
                        .is_some_and(|deadline| *deadline > now.elapsed)
                    && s.grants
                        .iter()
                        .any(|g| g.id == r.grant && !g.revoked && g.expires_ms > now.unix_ms)
            })
        });
    }

    fn save(&mut self) -> Result<()> {
        if let Err(error) = self.storage.save(&self.state) {
            self.disabled = true;
            self.readers.clear();
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
