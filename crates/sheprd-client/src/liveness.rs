//! A request, from the caller's handle, that an SSH link prove itself now
//! rather than at its routine health check. Why the link is suspect sets how
//! long its probe may go unanswered.

use crate::{limits::PING_TIMEOUT, session::Pace};
use std::{
    sync::atomic::{AtomicU8, Ordering},
    time::Duration,
};

/// Why a link may have died unnoticed, weakest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Suspicion {
    /// The network changed under the link. The connection has often
    /// survived, and after a stall TCP may take seconds to retransmit, so the
    /// probe is answered in a routine ping's time.
    NetworkChanged = 1,
    /// The machine slept. A link usually dies across it, so it must answer
    /// within a few seconds.
    Slept = 2,
}

impl Suspicion {
    /// How long the probe may go unanswered before the link is dropped.
    pub(crate) fn timeout(self, pace: Pace) -> Duration {
        match self {
            Self::NetworkChanged => PING_TIMEOUT,
            Self::Slept => pace.liveness,
        }
    }
}

/// The strongest suspicion asked for and not yet taken by the worker.
#[derive(Debug, Default)]
pub(crate) struct Request(AtomicU8);

impl Request {
    pub(crate) fn ask(&self, suspicion: Suspicion) {
        self.0.fetch_max(suspicion as u8, Ordering::AcqRel);
    }

    pub(crate) fn pending(&self) -> bool {
        self.0.load(Ordering::Acquire) != 0
    }

    pub(crate) fn take(&self) -> Option<Suspicion> {
        match self.0.swap(0, Ordering::AcqRel) {
            0 => None,
            1 => Some(Suspicion::NetworkChanged),
            _ => Some(Suspicion::Slept),
        }
    }
}
