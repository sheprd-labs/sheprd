//! Joining a fleet whose history is too large for one `fleet apply`: with
//! no fleet yet, the history of each fleet being handed over is kept as
//! far as it arrived, saved in `fleet/joining/` (`store`), and answered
//! from (`Fleet::apply`) so the member sends the rest. Kept per fleet, so
//! two members handing over different fleets at once never take or
//! overwrite each other's history, and at most [`MAX_FLEETS`] at once.
//! None grants anything until it admits this computer, or shares it as a
//! host the fleet holds; then it becomes the log and the rest are dropped.

use super::{Error, Fleet, Inner, Result, Scrubbed, store};
use crate::control::fleet::{Body, FleetId, Log, Op, OpId, sync};
use std::{collections::BTreeMap, sync::atomic::Ordering};

/// Histories kept at once; past it, the one touched longest ago goes.
pub(crate) const MAX_FLEETS: usize = 4;

/// The history of one fleet being joined.
#[derive(Debug)]
pub(crate) struct Joining {
    pub(crate) log: Log,
    /// When an operation last arrived for it, or the daemon loaded it.
    pub(crate) touched_ms: u64,
}

/// The fleet `op` belongs to: the one it founds, or the one it names.
pub(crate) fn fleet_of(op: &Op) -> FleetId {
    match op.body() {
        Body::Found(_) => FleetId(op.id().0),
        _ => op.unsigned().fleet,
    }
}

impl Fleet {
    /// Adds `ops` to the histories `inner` keeps of the fleets they belong
    /// to, with no log yet. The first history that admits this computer,
    /// or shares it as a host and holds `caller`, the heads of the member
    /// handing it over, is the log, returned with how many of its
    /// operations were refused; the others are dropped. `None` while none
    /// does. Answers are cut by bytes, so a share's `HostAdd` may arrive
    /// before the `Access` denials signed with it: a shared host waits for
    /// the caller's whole log, or it would let in computers left out. A history taken because it shares this computer is recorded
    /// (`fleet/hosted`), so only such a computer ever acts as a host
    /// without being a member. Blocking.
    pub(super) fn join_parts(
        &self,
        inner: &mut Inner,
        ops: Vec<Op>,
        caller: &[OpId],
        rejoin: Option<FleetId>,
        now_ms: u64,
    ) -> Result<Option<(Log, usize)>> {
        let mut by_fleet: BTreeMap<FleetId, Vec<Op>> = BTreeMap::new();
        for op in ops {
            by_fleet.entry(fleet_of(&op)).or_default().push(op);
        }
        let mut refused = BTreeMap::new();
        let mut approve_host = None;
        for (fleet, ops) in by_fleet {
            let explicit_host = rejoin == Some(fleet);
            let blocked = inner
                .scrubbed
                .is_some_and(|scrubbed| scrubbed.blocks_host(fleet));
            if blocked && !explicit_host {
                refused.insert(fleet, ops.len());
                continue;
            }
            let count = ops.len();
            let kept = inner.joining.remove(&fleet).map(|joining| joining.log);
            match join(kept, ops, now_ms) {
                Ok((log, failed)) => {
                    refused.insert(fleet, failed);
                    if explicit_host && blocked {
                        approve_host = Some(fleet);
                    }
                    inner.joining.insert(
                        fleet,
                        Joining {
                            log,
                            touched_ms: now_ms,
                        },
                    );
                }
                // Nothing that founds it, here or kept: dropped.
                Err(Error::NotAdmitted) => {
                    refused.insert(fleet, count);
                }
                Err(error) => return Err(error),
            }
        }
        while inner.joining.len() > MAX_FLEETS {
            let Some(oldest) = inner
                .joining
                .iter()
                .min_by_key(|(_, joining)| joining.touched_ms)
                .map(|(fleet, _)| *fleet)
            else {
                break;
            };
            inner.joining.remove(&oldest);
        }
        let admitted = inner
            .joining
            .iter()
            .filter(|(fleet, _)| {
                !inner
                    .scrubbed
                    .is_some_and(|scrubbed| scrubbed.blocks_member(**fleet))
            })
            .find(|(_, joining)| sync::admits(&joining.log, &self.me))
            .map(|(fleet, _)| (*fleet, false));
        let shared = || {
            inner
                .joining
                .iter()
                .filter(|(fleet, _)| {
                    rejoin == Some(**fleet)
                        || !inner
                            .scrubbed
                            .is_some_and(|scrubbed| scrubbed.blocks_host(**fleet))
                })
                .find(|(_, joining)| {
                    !caller.is_empty()
                        && caller.iter().all(|head| joining.log.get(head).is_some())
                        && self
                            .host
                            .is_some_and(|host| sync::shares(&joining.log, &host))
                })
                .map(|(fleet, _)| (*fleet, true))
        };
        let Some((fleet, hosted)) = admitted.or_else(shared) else {
            if let Some(fleet) = approve_host
                && inner.joining.contains_key(&fleet)
            {
                let next = Scrubbed::ExceptHost(fleet);
                if let Err(error) = store::save_scrubbed(&self.state_dir, next) {
                    inner.joining.remove(&fleet);
                    return Err(error);
                }
                inner.scrubbed = Some(next);
            }
            return Ok(None);
        };
        let Some(joining) = inner.joining.remove(&fleet) else {
            return Ok(None);
        };
        inner.joining.clear();
        if hosted {
            store::save_hosted(&self.state_dir, &fleet.to_string())?;
            self.hosted.store(true, Ordering::SeqCst);
        }
        Ok(Some((
            joining.log,
            refused.get(&fleet).copied().unwrap_or(0),
        )))
    }
}

/// The history of a fleet being joined, `joining` so far with `ops` added:
/// a founding operation starts it anew unless it founded `joining`, and
/// with neither there is nothing to join (`Error::NotAdmitted`).
fn join(joining: Option<Log>, mut ops: Vec<Op>, now_ms: u64) -> Result<(Log, usize)> {
    let found = ops
        .iter()
        .position(|op| matches!(op.body(), Body::Found(_)));
    let mut log = match (joining, found) {
        (Some(log), Some(found)) if log.fleet().0 == ops[found].id().0 => {
            ops.swap_remove(found);
            log
        }
        (_, Some(found)) => Log::found(ops.swap_remove(found))?,
        (Some(log), None) => log,
        (None, None) => return Err(Error::NotAdmitted),
    };
    let refused = log
        .insert_all(ops, now_ms)
        .iter()
        .filter(|(_, result)| result.is_err())
        .count();
    Ok((log, refused))
}
