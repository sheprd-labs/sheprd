//! How this daemon's APNs pushes leave it: not at all, with the user's own
//! APNs key straight to Apple, or through the push relay for a computer
//! with no key (`docs/apns-relay-plan.md`). The own key wins when both are
//! set up, since it involves no one else.
//!
//! Each mode reaches only its own kind of [`Recipient`]: a key reaches
//! tokens, the relay reaches handles. Through the relay every payload is
//! sealed to the phone ([`SealedPush`]), Live Activities included.
//!
//! A relay outage must not stall the notifier: once a relay could not be
//! reached, pushes through it are dropped at once for [`OUTAGE_MS`], so
//! alerts fall back to Web Push and activity updates are not queued.

use super::{
    Live, Outcome, Recipient, Result, Sender,
    alert::{self, Sealed},
    recipient::Handle,
    seal,
};
use p256::PublicKey;
use sheprd_apns::{RelayClient, RelayHost, RelayOutcome, SealedPush};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

/// How long pushes through the relay are dropped after it could not be
/// reached, each try having already waited through its retries.
pub(crate) const OUTAGE_MS: u64 = 60 * 1000;

/// What came of one push, as the caller acts on it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Sent {
    Delivered,
    /// The token or handle will never work again: forget it.
    Gone,
    /// Too many pushes for this phone for now.
    Limited,
    /// Refused, with APNs's status and reason. The relay gives no reason,
    /// and its status stands for one: 400 for a push it could not read, 403
    /// for a handle it no longer takes, which only the phone's next
    /// registration replaces.
    Refused {
        status: u16,
        reason: Option<String>,
    },
    /// APNs or the relay could not be reached, or the relay is out and the
    /// push was dropped.
    Unreachable,
    /// Nothing went out: the push could not be built or sealed, or this
    /// mode cannot reach the recipient.
    Unsent,
}

impl From<sheprd_apns::Result<Outcome>> for Sent {
    fn from(outcome: sheprd_apns::Result<Outcome>) -> Self {
        match outcome {
            Ok(Outcome::Delivered) => Self::Delivered,
            Ok(Outcome::Gone) => Self::Gone,
            Ok(Outcome::RateLimited) => Self::Limited,
            Ok(Outcome::Rejected { reason, status }) => Self::Refused { status, reason },
            Err(_) => Self::Unreachable,
        }
    }
}

impl From<RelayOutcome> for Sent {
    fn from(outcome: RelayOutcome) -> Self {
        let refused = |status| Self::Refused {
            status,
            reason: None,
        };
        match outcome {
            RelayOutcome::Delivered => Self::Delivered,
            RelayOutcome::Gone => Self::Gone,
            RelayOutcome::Limited => Self::Limited,
            RelayOutcome::Invalid => refused(400),
            RelayOutcome::Stale => refused(403),
            RelayOutcome::Unavailable => Self::Unreachable,
        }
    }
}

pub(crate) enum Delivery {
    /// No key and no relay: chat changes send no APNs push.
    Off,
    OwnKey(Box<Sender>),
    Relay(Box<Relays>),
}

impl Delivery {
    /// The user's own key in `state_dir` when there is one, else the relay
    /// when `push_relay` allows it. Blocking file I/O.
    pub(crate) fn load(state_dir: &Path, push_relay: bool) -> Result<Self> {
        if let Some(sender) = Sender::load(state_dir)? {
            return Ok(Self::OwnKey(Box::new(sender)));
        }
        if push_relay {
            return Ok(Self::Relay(Box::new(Relays::new()?)));
        }
        Ok(Self::Off)
    }

    pub(crate) fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    /// As the log names it.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::OwnKey(_) => "own key",
            Self::Relay(_) => "relay",
        }
    }

    /// As `POST /api/apns` tells the phone, so its Settings can say how
    /// this computer delivers: `None` when it sends nothing.
    pub(crate) fn wire_name(&self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::OwnKey(_) => Some("own_key"),
            Self::Relay(_) => Some("push_relay"),
        }
    }

    /// Whether this mode can send to `recipient`.
    pub(crate) fn reaches(&self, recipient: &Recipient) -> bool {
        matches!(
            (self, recipient),
            (Self::OwnKey(_), Recipient::Token(_)) | (Self::Relay(_), Recipient::Handle(_))
        )
    }

    /// Sends `sealed`, sealed to `key`, to `recipient` under `tag` as its
    /// collapse id, at `now_ms`.
    pub(crate) async fn alert(
        &self,
        recipient: &Recipient,
        key: &PublicKey,
        sealed: Sealed,
        tag: &str,
        now_ms: u64,
    ) -> Sent {
        match (self, recipient) {
            (Self::OwnKey(sender), Recipient::Token(token)) => {
                let Ok(push) = alert::push(sealed, tag, |plaintext| {
                    Ok(seal::seal(key, plaintext, seal::INFO)?)
                }) else {
                    return Sent::Unsent;
                };
                sender.post(token, &push, now_ms / 1000).await.into()
            }
            (Self::Relay(relays), Recipient::Handle(handle)) => {
                if relays.is_down(handle.environment(), now_ms) {
                    return Sent::Unreachable;
                }
                let clear = alert::clear(&sealed.pane, tag);
                let Ok(push) = SealedPush::build(key, seal::INFO, &clear, |sealer| {
                    alert::push(sealed, tag, |plaintext| Ok(sealer.seal(plaintext)?))
                }) else {
                    return Sent::Unsent;
                };
                relays.post(handle, &push, now_ms).await
            }
            _ => Sent::Unsent,
        }
    }

    /// Sends `live` to `recipient` at `now_ms`. Through the relay it is
    /// sealed to `key`, the phone's notify key, and goes nowhere without
    /// one; an update during a relay outage is dropped.
    pub(crate) async fn live(
        &self,
        recipient: &Recipient,
        key: Option<&PublicKey>,
        live: &Live,
        now_ms: u64,
    ) -> Sent {
        let now_s = now_ms / 1000;
        match (self, recipient) {
            (Self::OwnKey(sender), Recipient::Token(token)) => {
                let Ok(push) = live.push(now_s) else {
                    return Sent::Unsent;
                };
                sender.post(token, &push, now_s).await.into()
            }
            (Self::Relay(relays), Recipient::Handle(handle)) => {
                let Some(key) = key else {
                    return Sent::Unsent;
                };
                if relays.is_down(handle.environment(), now_ms) {
                    return Sent::Unreachable;
                }
                let Ok(push) = live.sealed_push(key, now_s) else {
                    return Sent::Unsent;
                };
                relays.post(handle, &push, now_ms).await
            }
            _ => Sent::Unsent,
        }
    }
}

/// The two relays, one per APNs environment. Each keeps its own outage, so
/// a sandbox relay that is down never holds back production pushes.
pub(crate) struct Relays {
    production: Relay,
    sandbox: Relay,
}

struct Relay {
    client: RelayClient,
    /// Until when pushes are dropped, in ms since the Unix epoch.
    down_until_ms: AtomicU64,
}

impl Relay {
    fn new(host: RelayHost) -> sheprd_apns::Result<Self> {
        Ok(Self {
            client: RelayClient::new(host)?,
            down_until_ms: AtomicU64::new(0),
        })
    }
}

impl Relays {
    /// Opens no connection until a push goes out.
    pub(crate) fn new() -> sheprd_apns::Result<Self> {
        Ok(Self {
            production: Relay::new(RelayHost::Production)?,
            sandbox: Relay::new(RelayHost::Sandbox)?,
        })
    }

    fn relay(&self, host: RelayHost) -> &Relay {
        match host {
            RelayHost::Production => &self.production,
            RelayHost::Sandbox => &self.sandbox,
        }
    }

    pub(crate) fn is_down(&self, host: RelayHost, now_ms: u64) -> bool {
        now_ms < self.relay(host).down_until_ms.load(Ordering::Relaxed)
    }

    /// Drops pushes through `host` for [`OUTAGE_MS`] from `now_ms`.
    pub(crate) fn mark_down(&self, host: RelayHost, now_ms: u64) {
        self.relay(host)
            .down_until_ms
            .fetch_max(now_ms.saturating_add(OUTAGE_MS), Ordering::Relaxed);
    }

    /// Sends `push` to the relay `handle` names, or drops it while that
    /// relay is out.
    async fn post(&self, handle: &Handle, push: &SealedPush, now_ms: u64) -> Sent {
        let host = handle.environment();
        if self.is_down(host, now_ms) {
            return Sent::Unreachable;
        }
        let outcome = self.relay(host).client.post(handle.secret(), push).await;
        if outcome == RelayOutcome::Unavailable {
            self.mark_down(host, now_ms);
        }
        outcome.into()
    }
}
