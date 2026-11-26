//! Joining a fleet from a computer in none (`docs/relay-plan.md`, Fleet,
//! Joining): while this computer is in no fleet, each computer it reaches
//! over SSH is asked which fleet it is a member of, and Settings > Fleet
//! offers to join it. Joining runs both halves from here, as an invitation
//! from a member does from there: this computer's daemon signs its bundle
//! for that fleet, the member's daemon admits it over SSH (`fleet admit`),
//! and the member's log is carried back here one answer at a time.
//!
//! Reaching the member's account over SSH is what proves the person
//! controls it, as it is for an invitation, so only a machine whose host
//! key is checked strictly is offered: joining lets the fleet's computers
//! in here.

use super::{
    Error, Fleet, Link, Machine, Standing,
    flows::{MAX_PARTS, applied, daemon},
    plain::strict_target,
    remote,
};
use crate::remote::Supervisor;
use gpui::{App, BorrowAppContext as _};
use sheprd_remote_control::control::fleet::{
    View,
    sync::{Admission, Answer, Apply, Ask},
};
use std::{
    collections::BTreeMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

/// How long a pairing's fleet step counts as running at most: the claim,
/// the other computer's Allow, and every retry of [`super::bluetooth::paired`].
pub(crate) const PAIRING_HOLD: Duration = Duration::from_secs(15 * 60);

/// A fleet this computer may join, and the reached members, by id, it may
/// join through: never empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offered {
    pub(crate) offer: Offer,
    pub(crate) through: Vec<(String, Machine)>,
}

impl Offered {
    /// The members to join through, `first` (an id) leading when it is
    /// one of them, the rest in their order.
    pub(crate) fn through_first(self, first: Option<&str>) -> Vec<(String, Machine)> {
        let mut through = self.through;
        if let Some(at) = through
            .iter()
            .position(|(id, _)| Some(id.as_str()) == first)
        {
            through[..=at].rotate_right(1);
        }
        through
    }
}

/// A fleet a reached computer is a member of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offer {
    /// Its id, hex.
    pub(crate) fleet: String,
    /// How many members and hosts it has.
    pub(crate) members: usize,
    pub(crate) hosts: usize,
}

impl Offer {
    /// What `view`, a reached computer's fleet, offers: nothing unless that
    /// computer is a member of it, as a shared host cannot admit anyone.
    pub(crate) fn of(view: &View) -> Option<Self> {
        view.members
            .iter()
            .any(|member| member.id == view.me)
            .then(|| Self {
                fleet: view.fleet.clone(),
                members: view.members.len(),
                hosts: view.hosts.len(),
            })
    }
}

impl Fleet {
    /// The fleets this computer, in none, may join, each with the reached
    /// members it may join through, by id.
    pub(crate) fn offers(cx: &App) -> Vec<Offered> {
        cx.try_global::<Self>()
            .map(|this| this.offered(this.reached(Instant::now())))
            .unwrap_or_default()
    }

    /// The fleets among the machines `reached` offer, in the order of the
    /// first machine (by id) offering each.
    pub(super) fn offered(&self, reached: BTreeMap<String, Machine>) -> Vec<Offered> {
        if self.view.is_some() {
            return Vec::new();
        }
        let mut offered: Vec<Offered> = Vec::new();
        for (id, machine) in reached {
            let Some(offer) = self.links.get(&id).and_then(|link| link.offer.clone()) else {
                continue;
            };
            match offered
                .iter_mut()
                .find(|known| known.offer.fleet == offer.fleet)
            {
                Some(known) => known.through.push((id, machine)),
                None => offered.push(Offered {
                    offer,
                    through: vec![(id, machine)],
                }),
            }
        }
        offered
    }

    /// Whether a join or an invitation runs: another waits for it, as two
    /// at once could leave a fleet listing a computer that joined another.
    pub(crate) fn busy(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|this| !this.acting.is_empty() || this.holding(Instant::now()))
    }

    fn holding(&self, now: Instant) -> bool {
        self.pairing
            .is_some_and(|(_, since)| now.duration_since(since) < PAIRING_HOLD)
    }

    /// Holds the fleet for the Nearby pairing `attempt`, whose claim
    /// carries it, so no join, invitation, or other pairing runs beside
    /// it. False when another fleet change runs.
    pub(crate) fn hold_for_pairing(cx: &mut App, attempt: u64) -> bool {
        if !cx.has_global::<Self>() || Self::busy(cx) {
            return false;
        }
        cx.update_global::<Self, _>(|this, _| this.pairing = Some((attempt, Instant::now())));
        cx.refresh_windows();
        true
    }

    /// Ends the hold of the pairing `attempt`, if it still holds it.
    pub(crate) fn release_pairing(cx: &mut App, attempt: u64) {
        if !cx.has_global::<Self>() {
            return;
        }
        cx.update_global::<Self, _>(|this, _| {
            if this.pairing.is_some_and(|(held, _)| held == attempt) {
                this.pairing = None;
            }
        });
        cx.refresh_windows();
    }

    /// Asks every reached machine again, as Settings > Fleet opens, so a
    /// fleet founded elsewhere since shows without waiting.
    pub(crate) fn rediscover(cx: &mut App) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        if this.view.is_some() {
            return;
        }
        cx.update_global::<Self, _>(|this, _| {
            for link in this.links.values_mut() {
                link.restart();
            }
        });
        Self::tick(cx);
    }

    /// Asks each reached machine that is due which fleet it is in.
    /// Only while Remote control runs here: without it, this computer
    /// could not join anyway.
    pub(super) fn discover_due(cx: &mut App, now: Instant) {
        if Supervisor::client(cx).is_none() {
            return;
        }
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        let due: Vec<(String, Machine)> = this
            .reached(now)
            .into_iter()
            .filter(|(id, _)| {
                !this.acting.contains(id)
                    && this.links.get(id).is_none_or(|link| link.due(&[], now))
            })
            .collect();
        for (id, machine) in due {
            discover(cx, id, machine);
        }
    }
}

/// Asks `machine` which fleet it is a member of, for this computer in
/// none.
fn discover(cx: &mut App, id: String, machine: Machine) {
    Fleet::set_link(cx, &id, |link| {
        link.running = true;
        link.last = Some(Instant::now());
    });
    let task = cx.background_executor().spawn(async move {
        match strict_target(&machine.target) {
            Ok(_) => {}
            Err(Error::Unchecked) => return Ok((Standing::Unchecked, None)),
            Err(_) => return Ok((Standing::Outside, None)),
        }
        match remote::status(&machine.target, &AtomicBool::new(false)) {
            Ok(view) => Ok((Standing::Outside, view.as_ref().and_then(Offer::of))),
            Err(Error::NoProgram) => Ok((Standing::NoDaemon, None)),
            Err(error) => Err::<_, Error>(error),
        }
    });
    cx.spawn(async move |cx| {
        let outcome = task.await;
        cx.update(|cx| {
            Fleet::set_link(cx, &id, |link| {
                link.running = false;
                match outcome {
                    Ok((standing, offer)) => {
                        link.standing = standing;
                        link.offer = offer;
                    }
                    Err(error) => {
                        tracing::debug!(%error, "Could not ask a machine for its fleet");
                        link.standing = Standing::Failed;
                        link.offer = None;
                    }
                }
            });
            cx.refresh_windows();
        });
    })
    .detach();
}

/// Joins the fleet `machine` is a member of (a person here asked for it):
/// this computer signs its bundle for that fleet, the member admits it
/// over SSH, and the member's log is carried back here, which admits it.
pub(crate) fn join(
    cx: &mut App,
    id: String,
    machine: Machine,
    done: impl FnOnce(Option<String>, &mut App) + 'static,
) {
    let Some(client) = Supervisor::client(cx) else {
        done(Some(Error::NoDaemon.to_string()), cx);
        return;
    };
    // One fleet change at a time, and none once this computer is in one.
    if Fleet::busy(cx) || Fleet::view(cx).is_some() {
        done(Some(Error::Busy.to_string()), cx);
        return;
    }
    Fleet::set_acting(cx, &id, true);
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let outcome = async {
            let target = machine.target.clone();
            // Asked again, so the fleet joined is the one it is in now.
            let offer = executor
                .spawn(async move {
                    strict_target(&target)?;
                    remote::status(&target, &AtomicBool::new(false))
                })
                .await?
                .as_ref()
                .and_then(Offer::of)
                .ok_or(Error::NotMember)?;
            let fleet = offer.fleet;
            let admission = match daemon(
                &client,
                Ask::Bundle {
                    fleet: fleet.clone(),
                },
            )
            .await?
            {
                Answer::Bundle { bundle, signature } => Admission { bundle, signature },
                _ => return Err(Error::Unexpected),
            };
            let target = machine.target.clone();
            executor
                .spawn(async move { remote::admit(&target, &admission, &AtomicBool::new(false)) })
                .await?;
            take_log(&client, &executor, &machine, &fleet).await
        }
        .await;
        cx.update(|cx| {
            Fleet::set_acting(cx, &id, false);
            // In the fleet now: every machine is asked again, as a member.
            if outcome.is_ok() {
                cx.update_global::<Fleet, _>(|this, _| {
                    this.links.values_mut().for_each(Link::restart);
                });
            }
            Fleet::refresh(cx);
            done(outcome.err().map(|error| error.to_string()), cx);
        });
    })
    .detach();
}

/// Carries `fleet`'s log from `machine`, a member that just admitted this
/// computer, one answer at a time: this computer keeps the history as far
/// as it arrived, and joins once it holds its admission.
async fn take_log(
    client: &crate::remote::supervisor::Client,
    executor: &gpui::BackgroundExecutor,
    machine: &Machine,
    fleet: &str,
) -> Result<(), Error> {
    let mut ours = Vec::new();
    for _ in 0..MAX_PARTS {
        let ask = Apply {
            fleet: Some(fleet.to_owned()),
            heads: ours,
            ..Apply::default()
        };
        let target = machine.target.clone();
        let part = executor
            .spawn(async move { remote::apply(&target, &ask, &AtomicBool::new(false)) })
            .await?;
        if part.ops.is_empty() {
            return Ok(());
        }
        let more = part.more;
        let kept = applied(
            client,
            Apply {
                fleet: Some(fleet.to_owned()),
                heads: part.heads,
                ops: part.ops,
                ..Apply::default()
            },
        )
        .await?;
        if !more {
            return Ok(());
        }
        ours = kept.heads;
    }
    Err(Error::TooLarge)
}

/// Marks a join or an invitation as running for `machine`, or not.
#[cfg(test)]
pub(crate) fn set_acting(cx: &mut App, machine: &str, acting: bool) {
    Fleet::set_acting(cx, machine, acting);
}

/// Sets the fleet `machine` offers, as a discovery that just ran would.
#[cfg(test)]
pub(crate) fn set_offer(cx: &mut App, machine: &str, offer: Option<Offer>) {
    Fleet::set_link(cx, machine, |link| link.offer = offer);
}
