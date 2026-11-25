//! Live Activity payloads for the push relay, sealed to the phone so neither
//! the relay nor APNs reads them (`docs/apns-relay-plan.md`, Live Activity
//! privacy). `content-state`, and `attributes` on a start, each become one
//! `sealed` field under [`seal::ACTIVITY_INFO`]; the widget extension opens
//! them with the key alerts use. Only what iOS itself needs stays readable:
//!
//! ```json
//! { "aps": {
//!     "timestamp": 1800000000, "event": "start",
//!     "attributes-type": "ChatActivityAttributes",
//!     "attributes": { "sealed": "BASE64URL" },
//!     "content-state": { "sealed": "BASE64URL" },
//!     "alert": { "title": "Sheprd", "body": "New activity" } } }
//! ```
//!
//! An update's alert is the same cover, and an end keeps its
//! `dismissal-date`. What is sealed is what the phone may see, after
//! `Live::redacted`, cut until the payload fits APNs's 4 KB: the earlier
//! steps go first, then the longest text is halved, and when nothing is left
//! to halve, the answer buttons go with the permission they answer.

use super::{
    Error, Event, Live, Push, PushType, Result,
    alert::{COVER_BODY, COVER_TITLE},
    payload::{ATTRIBUTES_TYPE, MAX_PAYLOAD},
    seal,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::PublicKey;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sheprd_apns::{SealedPush, Sealer};

/// Text no shorter than this is left alone when cutting to fit.
const MIN_TEXT: usize = 8;

/// Assigned before spawning the sender, so retries and reordered tasks retain
/// the host's original ordering. Unix microseconds survive daemon restarts.
pub(crate) fn revision() -> u64 {
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    let prior = LAST.update(Ordering::Relaxed, Ordering::Relaxed, |last| {
        now.max(last + 1)
    });
    now.max(prior + 1)
}

impl Live {
    pub(super) fn shared_push(&self, device: &PublicKey, revision: u64) -> Result<SealedPush> {
        use sha2::{Digest, Sha256};
        // Hash the random card key. A pane, machine identity or label must
        // never become a cleartext correlation key.
        let digest = Sha256::digest(self.pane.as_bytes());
        let run = u64::from_be_bytes(digest[..8].try_into().unwrap_or_default());
        let mut live = self.clone();
        loop {
            let built = SealedPush::build(device, seal::ACTIVITY_INFO, &[] as &[&str], |sealer| {
                let agents = live.state.detail.agents.as_ref();
                let end = matches!(live.event, Event::End { .. });
                let latest = agents
                    .and_then(|p| p.latest.as_ref())
                    .map(|label| sealed_field(sealer, label))
                    .transpose()?;
                let body = serde_json::to_vec(&json!({
                    "revision": revision, "run": run, "end": end,
                    "restored": live.pane.rsplit('/').next().is_some_and(|s| s.starts_with("restored-")),
                    "working": live.state.working, "waiting": live.state.waiting,
                    "question": live.state.phase == super::Phase::Question,
                    "question_revision": live.state.detail.question_revision,
                    "done": agents.map_or(usize::from(end), |p| p.done),
                    "total": agents.map_or(1, |p| p.total),
                    "state": sealed_field(sealer, &live.state)?, "latest": latest
                }))?;
                // Leave room to combine another member's encrypted label and
                // the relay's ActivityKit envelope, even on a start.
                if body.len() > 2850 {
                    return Err(Error::PayloadTooLarge { len: body.len() });
                }
                Ok(Push {
                    body,
                    priority: self.priority(),
                    collapse_id: None,
                    push_type: PushType::LiveActivity,
                })
            });
            match built {
                Err(Error::PayloadTooLarge { .. }) if live.shorten() => {}
                result => return result,
            }
        }
    }

    /// The push for this change at `now_s`, sealed to `device`, with its
    /// longest text cut until it fits.
    pub(crate) fn sealed_push(&self, device: &PublicKey, now_s: u64) -> Result<SealedPush> {
        // Everything else in the payload is a number or a sealed field.
        let clear = [COVER_TITLE, COVER_BODY, self.event.name(), ATTRIBUTES_TYPE];
        let mut live = self.clone();
        loop {
            match SealedPush::build(device, seal::ACTIVITY_INFO, &clear, |sealer| {
                live.sealed(sealer, now_s)
            }) {
                Err(Error::PayloadTooLarge { .. }) if live.shorten() => {}
                built => return built,
            }
        }
    }

    fn sealed(&self, sealer: &Sealer<'_>, timestamp_s: u64) -> Result<Push> {
        let cover = json!({ "title": COVER_TITLE, "body": COVER_BODY });
        let mut aps = Map::new();
        aps.insert("timestamp".into(), json!(timestamp_s));
        aps.insert("event".into(), json!(self.event.name()));
        aps.insert("content-state".into(), sealed_field(sealer, &self.state)?);
        match &self.event {
            Event::Start { attributes, .. } => {
                aps.insert("attributes-type".into(), json!(ATTRIBUTES_TYPE));
                aps.insert("attributes".into(), sealed_field(sealer, attributes)?);
                aps.insert("alert".into(), cover);
            }
            Event::Update { alert: Some(_) } => {
                aps.insert("alert".into(), cover);
            }
            Event::Update { alert: None } => {}
            Event::End { dismissal_s } => {
                aps.insert("dismissal-date".into(), json!(dismissal_s));
            }
        }
        let mut root = Map::new();
        root.insert("aps".into(), Value::Object(aps));
        let body = serde_json::to_vec(&Value::Object(root))?;
        if body.len() > MAX_PAYLOAD {
            return Err(Error::PayloadTooLarge { len: body.len() });
        }
        Ok(Push {
            body,
            collapse_id: None,
            priority: self.priority(),
            push_type: PushType::LiveActivity,
        })
    }

    /// Drops the earlier steps, else halves the longest text, else drops
    /// the answer buttons; `false` when nothing is left worth cutting.
    fn shorten(&mut self) -> bool {
        let state = &mut self.state;
        if !state.previous.is_empty() || !state.detail.previous_activities.is_empty() {
            state.previous.clear();
            state.detail.previous_activities.clear();
            return true;
        }
        let mut texts = vec![&mut state.step];
        texts.extend(state.detail.activity.as_mut());
        texts.extend(state.question.as_mut());
        texts.extend(state.summary.as_mut());
        texts.extend(state.detail.agents.as_mut().and_then(|a| a.latest.as_mut()));
        if let Some(focus) = &mut state.detail.focus {
            texts.extend([&mut focus.chat_title, &mut focus.workspace]);
        }
        if let Event::Start { attributes, .. } = &mut self.event {
            texts.extend([&mut attributes.chat_title, &mut attributes.workspace]);
        }
        let longest = texts
            .into_iter()
            .max_by_key(|text| text.len())
            .filter(|text| text.chars().count() > MIN_TEXT);
        if let Some(longest) = longest {
            let chars = longest.chars().count();
            let kept: String = longest.chars().take(chars / 2).collect();
            *longest = format!("{kept}\u{2026}");
            return true;
        }
        // Buttons whose names alone do not fit: the phone shows the question
        // without them, as a generic activity does, and answers in the app.
        if self.state.options.is_empty() {
            return false;
        }
        self.state.options.clear();
        self.state.permission = None;
        true
    }
}

/// `{"sealed": BASE64URL}` holding `value` as JSON.
fn sealed_field(sealer: &Sealer<'_>, value: &impl Serialize) -> Result<Value> {
    let sealed = sealer.seal(&serde_json::to_vec(value)?)?;
    Ok(json!({ "sealed": URL_SAFE_NO_PAD.encode(sealed) }))
}
