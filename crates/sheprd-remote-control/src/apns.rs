//! Live Activities and notifications on paired iPhones through the Apple
//! Push Notification service: the companion app in `ios/` shows one
//! activity per running chat on the lock screen and in the Dynamic Island,
//! which the daemon starts, updates, and ends with `liveactivity` pushes,
//! and chat notifications sealed to the device.
//!
//! The credentials, the HTTP/2 sender, the relay client, and the sealing
//! live in `sheprd_apns`. [`Delivery`] picks how pushes leave: the user's
//! own key (`STATE_DIR/apns/AuthKey.p8` and `config.json`) straight to
//! Apple, else the push relay when `[remote] push_relay` allows it
//! (`docs/apns-relay-plan.md`), else nothing.
//!
//! - [`decide`]: pure, from the same inputs as [`crate::push::decide`],
//!   whether a chat change starts, updates, or ends the chat's activity.
//! - `redact`: what one device's activity may say, from the computer's and
//!   the phone's choices, applied just before sending.
//! - [`Pacer`]: holds back step updates so a busy turn does not spend the
//!   phone's Live Activity budget.
//! - [`Registration`]: the push tokens one device sent, kept in the device
//!   store's opaque slot as JSON; each a token or a relay handle
//!   ([`Recipient`]).
//! - [`alert`]: chat notifications for the native app, sealed to the
//!   device with [`seal`] so APNs relays only ciphertext.
//! - `relayed`: Live Activity payloads sealed the same way, for the relay.
//! - [`Ledger`]: how the last push of each kind went, for Sheprd.
//!
//! A device only ever supplies hex tokens, or relay handles with the name
//! of an environment. The host is one of Apple's two, chosen by the user's
//! config, or one of the two relays compiled in, `push.sheprd.org` and
//! `push-sandbox.sheprd.org` for Xcode builds, chosen by the handle's
//! environment, so a paired device cannot make the daemon post anywhere
//! else.

pub(crate) mod alert;
mod decide;
mod delivery;
mod ledger;
mod pacer;
mod payload;
mod recipient;
mod redact;
mod relayed;
mod terminal;
mod tokens;

pub(crate) use decide::{computer_id, decide};
#[cfg(test)]
pub(crate) use delivery::Relays;
pub(crate) use delivery::{Delivery, Sent};
pub(crate) use ledger::{Kind, Ledger};
pub(crate) use pacer::Pacer;
#[cfg(test)]
pub(crate) use payload::ATTRIBUTES_TYPE;
pub(crate) use payload::{Alert, Attributes, ContentState, Event, Live, Phase};
#[cfg(test)]
pub(crate) use recipient::Handle;
pub(crate) use recipient::Recipient;
pub(crate) use sheprd_apns::{DeviceToken, Outcome, Priority, Push, PushType, Sender, seal};
pub(crate) use terminal::decide as decide_terminal;
#[cfg(test)]
pub(crate) use tokens::ActivityToken;
pub(crate) use tokens::Registration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Apns(#[from] sheprd_apns::Error),
    #[error("a push relay handle must be a key id and sealed base64url, at most 2048 bytes")]
    Handle,
    #[error("could not encode the APNs payload")]
    Json(#[from] serde_json::Error),
    #[error("the notification key is not a P-256 point in base64url")]
    NotifyKey,
    #[error("a Live Activity's pane must be 1 to 128 characters with no control characters")]
    Pane,
    #[error("the APNs payload is {len} bytes, more than {max}", max = payload::MAX_PAYLOAD)]
    PayloadTooLarge { len: usize },
    #[error("{len} Live Activity tokens, more than {max}", max = tokens::MAX_ACTIVITIES)]
    TooManyActivities { len: usize },
    // keep-sorted end
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// A sender with a made-up key and team, which never connects until it
/// sends, for other modules' tests.
#[cfg(test)]
#[allow(clippy::expect_used)]
pub(crate) fn sender_for_tests() -> Sender {
    let key = p256::ecdsa::SigningKey::from(
        p256::SecretKey::from_slice(&[7; 32]).expect("a valid scalar"),
    );
    let config = sheprd_apns::Config::from_parts(
        "ABCDE12345",
        "XYZ9876543",
        "so.pen.herdr.remote",
        true,
        key,
    )
    .expect("a valid config");
    Sender::new(config).expect("a sender")
}

#[cfg(test)]
mod tests;
