//! What pairing with another computer's Sheprd does for the fleet
//! (`docs/relay-plan.md`, Adding a nearby Sheprd from another network).
//! Once both screens show the code, this page knows both computers'
//! fleets: this one's from its daemon, the other's from the sealed offer
//! or the link. Pairing brings the two into one fleet whenever one of them
//! is in one, without a question here: the claim goes at once, and the
//! other computer's Allow carries the line that says so, ticked by
//! default. When neither is in a fleet, this computer founds one and
//! invites the other; when each is in its own, the two pair only.

use super::{NearbySetup, Step};
use crate::SheprdWindow;
use crate::fleet::{Error, Fleet, bluetooth};
use gpui::{App, Context};
use sheprd_remote_control::control::fleet::{
    View,
    bluetooth::{ClaimFleet, Hint},
};

/// What the claim asks of the fleet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ask {
    /// Nothing: both are in the same fleet, this computer's daemon has
    /// not said yet, or it holds a fleet it is no member of (shared as a
    /// host, or removed).
    Nothing,
    /// Neither is in a fleet: this computer founds one and invites the
    /// other into it.
    Found,
    /// This computer's fleet, for the other computer to join. One in
    /// another fleet names it and pairs only.
    Invite(Hint),
    /// The other computer's fleet, which this one, in none, joins.
    Join(String),
}

impl Ask {
    /// What the claim asks, from this computer's `view` and whether it is
    /// a member there, and the fleet the other computer named.
    pub(super) fn of(view: Option<&View>, member: bool, theirs: Option<&Hint>) -> Self {
        match (view, member, theirs) {
            (Some(view), true, Some(theirs)) if theirs.fleet.eq_ignore_ascii_case(&view.fleet) => {
                Self::Nothing
            }
            (Some(view), true, _) => Self::Invite(bluetooth::hint(view)),
            (Some(_), false, _) => Self::Nothing,
            (None, _, Some(theirs)) => Self::Join(theirs.fleet.clone()),
            (None, _, None) => Self::Found,
        }
    }
}

/// What this computer's claim asks with `theirs`, as the daemon last said.
pub(super) fn ask(cx: &App, theirs: Option<&Hint>) -> Ask {
    if !Fleet::read(cx) {
        return Ask::Nothing;
    }
    Ask::of(Fleet::view(cx), Fleet::member_of(cx).is_some(), theirs)
}

impl SheprdWindow {
    /// Hands the child its claim, with what it asks of the fleet. A join
    /// waits for this computer's daemon to sign its bundle, bound to the
    /// pairing whose token hashes to `pairing`, and a new fleet for the
    /// daemon to found it.
    pub(super) fn nearby_claim(
        &mut self,
        theirs: Option<&Hint>,
        pairing: String,
        cx: &mut Context<Self>,
    ) {
        let mut asked = ask(cx, theirs);
        let Some(attempt) = self.menu.nearby_setup.as_ref().map(|setup| setup.attempt) else {
            return;
        };
        // One fleet change at a time: a join or invitation from Settings,
        // or another window's pairing, could otherwise leave a fleet
        // listing this computer while it joins another.
        if asked != Ask::Nothing && !Fleet::hold_for_pairing(cx, attempt) {
            Fleet::pairing_done(cx, Some(Error::Busy.to_string()));
            asked = Ask::Nothing;
        }
        let Some(setup) = &self.menu.nearby_setup else {
            return;
        };
        match asked {
            Ask::Nothing => claim(setup, None),
            Ask::Invite(ours) => claim(setup, Some(ClaimFleet::Invite(ours))),
            Ask::Join(fleet) => {
                let this = cx.entity().downgrade();
                bluetooth::joining(cx, fleet, pairing, move |joining, cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.nearby_claim_with(attempt, joining.map(ClaimFleet::Join), cx);
                    });
                });
            }
            Ask::Found => {
                let this = cx.entity().downgrade();
                bluetooth::found(cx, move |founded, cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.nearby_claim_with(attempt, founded.map(ClaimFleet::Invite), cx);
                    });
                });
            }
        }
    }

    /// Sends the claim with what the daemon prepared for `attempt`: a
    /// bundle it signed, or the fleet it founded. One it could not
    /// prepare, such as on a computer that listens on no relay, pairs
    /// only, and Settings > Fleet says why.
    pub(super) fn nearby_claim_with(
        &mut self,
        attempt: u64,
        fleet: Result<ClaimFleet, Error>,
        cx: &mut Context<Self>,
    ) {
        // Cancelled, failed, or another computer picked meanwhile: the
        // child that waited for this is gone, and so is its hold.
        let current = self.menu.nearby_setup.as_ref().is_some_and(|setup| {
            setup.attempt == attempt && matches!(setup.step, Step::Code { .. })
        });
        if !current {
            Fleet::release_pairing(cx, attempt);
            return;
        }
        let fleet = match fleet {
            Ok(fleet) => Some(fleet),
            Err(error) => {
                tracing::debug!(%error, "Pairing without bringing the fleet along");
                Fleet::pairing_done(cx, Some(error.to_string()));
                Fleet::release_pairing(cx, attempt);
                None
            }
        };
        if let Some(setup) = &self.menu.nearby_setup {
            claim(setup, fleet);
        }
        cx.notify();
    }
}

/// Hands the child its claim.
fn claim(setup: &NearbySetup, fleet: Option<ClaimFleet>) {
    if let Some(pairing) = &setup.pairing {
        pairing.claim(fleet);
    }
}

#[cfg(test)]
mod tests;
