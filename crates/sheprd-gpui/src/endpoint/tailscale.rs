//! What an endpoint remembers about the Tailscale check it is waiting on.
//!
//! A check belongs to the connection that printed it, and every retry replaces
//! that connection and the `live` state that came with it. The typed failure
//! never fills on the transport path at all, so a surface reading `live` alone
//! would show the address for one frame and then nothing. What is worth keeping
//! is the check itself, in one place every surface reads the same way.

use super::LiveState;
use crate::tailscale;
use sheprd_client::TailscaleCheck;

/// A pending check, and with it the address that answers it.
#[derive(Default)]
pub(super) struct PendingCheck(Option<TailscaleCheck>);

impl PendingCheck {
    /// Remember what the last attempt said, in its failure or in the sentence
    /// it left. An attempt that has said nothing yet leaves what is remembered
    /// alone — a retry empties the live error while the check is still pending —
    /// but an attempt that answers with anything else ends the check: the window
    /// must show what the host says now, not an address that no longer applies.
    pub(super) fn observe(&mut self, live: &LiveState, outage: Option<&str>) {
        let sentence = live.error.as_deref().or(outage);
        if let Some(found) = tailscale::check(live.ssh_failure.as_ref(), sentence) {
            self.0 = Some(found);
        } else if live.ssh_failure.is_some() || live.error.is_some() {
            self.0 = None;
        }
    }

    /// The pending check: from the live failure while the transport carries it,
    /// from what this endpoint remembered otherwise.
    pub(super) fn pending<'a>(&'a self, live: &'a LiveState) -> Option<&'a TailscaleCheck> {
        tailscale::pending(live.ssh_failure.as_ref()).or(self.0.as_ref())
    }

    /// The host is no longer waiting on a check, or the connection it belonged
    /// to is gone.
    pub(super) fn clear(&mut self) {
        self.0 = None;
    }
}

#[cfg(test)]
mod tests;
