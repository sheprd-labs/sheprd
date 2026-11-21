//! What [`super::Request::PushStatus`] answers: how this daemon's iPhone
//! pushes leave it (`docs/apns-relay-plan.md`), and how the last push of
//! each kind went since it started. It names no token, handle, device, or
//! content: only the mode, a time, and an outcome.

use serde::{Deserialize, Serialize};

/// How APNs pushes leave the daemon.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// No APNs key and no push relay: phones get Web Push only.
    #[default]
    Off,
    /// The user's own APNs key, straight to Apple. It wins over the push relay.
    OwnKey,
    /// Through the Sheprd push relay (`push.sheprd.org`), sealed to the
    /// phone.
    PushRelay,
    /// A mode this Sheprd does not know, from a newer daemon.
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PushStatus {
    pub mode: Mode,
    /// The last chat alert or test push; absent until one went out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<Last>,
    /// The last Live Activity start, update, or end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_activity: Option<Last>,
}

/// One push: when, and what came of it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Last {
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// What came of a push.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum Outcome {
    Delivered,
    /// The phone's token or handle no longer works and was forgotten; the
    /// app registers again when it opens.
    Gone,
    /// Too many pushes for that phone for now.
    Limited,
    /// Refused, with APNs's status and reason; the push relay gives none.
    Refused {
        status: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// APNs or the push relay could not be reached.
    Unreachable,
    /// The push could not be built or sealed, so nothing went out.
    Unsent,
    /// An outcome this Sheprd does not know, from a newer daemon.
    #[serde(other)]
    Other,
}

impl Outcome {
    /// Whether the push reached Apple.
    pub fn worked(&self) -> bool {
        matches!(self, Self::Delivered)
    }
}
