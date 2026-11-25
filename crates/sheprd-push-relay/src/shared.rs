//! One ephemeral coordinator per opted-in phone generation. Serialize APNs
//! delivery per card, including start acknowledgement and late token binding.
use crate::relay::Reply;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sheprd_apns::{Priority, Push, PushType};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, PoisonError},
};
use tokio::sync::Mutex as AsyncMutex;

const LIFETIME: u64 = 8 * 3600;
type Entry = Arc<AsyncMutex<Card>>;

#[derive(Default)]
pub(crate) struct Groups(Mutex<HashMap<String, (u64, Entry)>>);

impl Groups {
    pub(crate) fn entry(&self, group: &str, now: u64) -> Option<Entry> {
        let mut entries = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|_, (last, card)| {
            now.saturating_sub(*last) < LIFETIME || Arc::strong_count(card) > 1
        });
        if entries.len() >= 1024 && !entries.contains_key(group) {
            return None;
        }
        let (last, entry) = entries.entry(group.to_owned()).or_default();
        *last = now;
        Some(entry.clone())
    }
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Cipher {
    sealed: String,
}
impl Cipher {
    fn valid(&self, max: usize) -> bool {
        (1..=max).contains(&self.sealed.len())
            && self
                .sealed
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    revision: u64,
    run: u64,
    #[serde(default)]
    restored: bool,
    end: bool,
    question: bool,
    question_revision: Option<u64>,
    /// Host focus can move between several pending panes. Retain the alert
    /// receipts across focus changes until the run stops waiting.
    #[serde(skip)]
    alerted: VecDeque<u64>,
    working: u32,
    waiting: u32,
    done: u32,
    total: u32,
    state: Cipher,
    latest: Option<Cipher>,
    #[serde(skip)]
    arrival: u64,
}

#[derive(Default)]
pub(crate) struct Card {
    reports: HashMap<String, Report>,
    revisions: HashMap<String, u64>,
    carried: u64,
    latest: Option<(String, Cipher)>,
    nonce: String,
    started: bool,
    token_gone: bool,
    dirty: bool,
    timestamp: u64,
    arrival: u64,
    pub(crate) token: Option<SecretString>,
}

impl Card {
    pub(crate) fn report(&mut self, slot: &str, text: &str) -> Result<Option<bool>, Reply> {
        let mut r: Report = serde_json::from_str(text).map_err(|_| Reply::Invalid)?;
        if !r.state.valid(2850)
            || r.latest.as_ref().is_some_and(|x| !x.valid(800))
            || r.total == 0
            || r.done > r.total
            || (r.end && r.done != r.total)
        {
            return Err(Reply::Invalid);
        }
        if self
            .revisions
            .get(slot)
            .is_some_and(|rev| *rev >= r.revision)
        {
            return self.action();
        }
        if self.revisions.len() >= 32 && !self.revisions.contains_key(slot) {
            return Err(Reply::Limited);
        }
        // A finish can trail its card into an unrelated host's next session.
        if r.end && !self.reports.contains_key(slot) {
            self.revisions.insert(slot.into(), r.revision);
            return Ok(None);
        }
        if self.token_gone && self.ended() && !r.end {
            self.retire();
        }
        let mut previous = self.reports.get_mut(slot);
        if let Some(old) = previous.as_deref_mut()
            && r.question
            && old.question
            && r.run == old.run
        {
            r.alerted = std::mem::take(&mut old.alerted);
        }
        let previous = previous.as_deref();
        if let Some(previous) = previous.filter(|p| p.run != r.run) {
            // A restarted daemon can only reconstruct currently observable
            // panes. Credit earlier completions missing from its new snapshot.
            let completed = if r.restored && !previous.end {
                previous
                    .total
                    .saturating_sub(r.total)
                    .max(previous.done.saturating_sub(r.done))
            } else {
                previous.total
            };
            self.carried += u64::from(completed);
        }
        let redacted =
            r.latest.is_none() && self.latest.as_ref().is_some_and(|(owner, _)| owner == slot);
        if redacted || r.end || r.done > previous.filter(|p| p.run == r.run).map_or(0, |p| p.done) {
            self.latest = r.latest.clone().map(|label| (slot.into(), label));
        }
        self.revisions.insert(slot.into(), r.revision);
        self.arrival += 1;
        r.arrival = self.arrival;
        self.reports.insert(slot.into(), r);
        self.dirty = true;
        self.action()
    }

    fn action(&mut self) -> Result<Option<bool>, Reply> {
        if !self.dirty {
            return Ok(None);
        }
        if self.nonce.is_empty() || (self.token_gone && !self.ended()) {
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes).map_err(|_| Reply::Unavailable)?;
            self.nonce = bytes.iter().map(|b| format!("{b:02x}")).collect();
            self.started = false;
            self.token_gone = false;
        }
        Ok((!self.started || self.token.is_some()).then_some(!self.started))
    }

    pub(crate) fn bind(&mut self, nonce: &str, token: SecretString) -> Result<Option<bool>, Reply> {
        if self.nonce != nonce {
            return Err(Reply::Gone);
        }
        // Receiving the nonce from the phone proves APNs started the card,
        // including when the gateway lost the acknowledgement.
        self.started = true;
        self.token_gone = false;
        if self
            .token
            .as_ref()
            .is_none_or(|old| old.expose_secret() != token.expose_secret())
        {
            self.token = Some(token);
            self.dirty = true;
        }
        self.action()
    }

    fn ended(&self) -> bool {
        self.reports.values().all(|r| r.end)
    }

    pub(crate) fn push(&mut self, group: &str, start: bool, now: u64) -> Result<Push, Reply> {
        let (slot, focus) = self
            .reports
            .iter()
            .max_by_key(|(_, r)| (!r.end, r.question, r.arrival))
            .ok_or(Reply::Invalid)?;
        let (done, total) = self
            .reports
            .values()
            .fold((self.carried, self.carried), |(d, t), r| {
                (d + u64::from(r.done), t + u64::from(r.total))
            });
        let (working, waiting) = self
            .reports
            .values()
            .filter(|r| !r.end)
            .fold((0u64, 0u64), |(w, q), r| {
                (w + u64::from(r.working), q + u64::from(r.waiting))
            });
        let event = if start {
            "start"
        } else if self.ended() {
            "end"
        } else {
            "update"
        };
        self.timestamp = now.max(self.timestamp + 1);
        let mut aps = json!({"event":event,"timestamp":self.timestamp,"content-state":{"shared":{
            "slot":slot, "sealed":focus.state.sealed, "done":done, "total":total,"working":working,"waiting":waiting,
            "latest":self.latest.as_ref().map(|(slot,label)| json!({"slot":slot,"sealed":label.sealed}))
        }}});
        if start {
            aps["input-push-token"] = json!(1);
            aps["attributes-type"] = json!("ChatActivityAttributes");
            // Only the phone and sealed capabilities know the group secret.
            // The APNs-visible marker cannot authorize another attested phone.
            let marker: String = Sha256::digest(group.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            aps["attributes"] = json!({"shared":{"group":marker,"card":self.nonce}});
        }
        if start
            || (focus.question
                && !focus
                    .alerted
                    .contains(&focus.question_revision.unwrap_or_default()))
        {
            aps["alert"] = json!({"title":"Sheprd","body":"New activity"});
        }
        if event == "end" {
            aps["dismissal-date"] = json!(now + 60);
        }
        let body = serde_json::to_vec(&json!({"aps":aps})).map_err(|_| Reply::Invalid)?;
        if body.len() > sheprd_apns::MAX_PAYLOAD {
            return Err(Reply::Invalid);
        }
        Ok(Push {
            body,
            collapse_id: Some(self.nonce.clone()),
            push_type: PushType::LiveActivity,
            priority: if start || focus.question || event == "end" {
                Priority::High
            } else {
                Priority::Low
            },
        })
    }

    pub(crate) fn delivered(&mut self, start: bool) {
        if let Some(focus) = self
            .reports
            .values_mut()
            .filter(|r| !r.end && r.question)
            .max_by_key(|r| r.arrival)
        {
            let revision = focus.question_revision.unwrap_or_default();
            if !focus.alerted.contains(&revision) {
                // Bound receipts even for a host that keeps asking new
                // questions without ever reporting a quiet moment.
                if focus.alerted.len() >= 256 {
                    focus.alerted.pop_front();
                }
                focus.alerted.push_back(revision);
            }
        }
        self.dirty = false;
        self.started = true;
        if !start && self.ended() {
            self.retire();
        }
    }

    pub(crate) fn lost_token(&mut self) {
        // APNs may have rotated the token before the phone binds its replacement.
        self.token = None;
        self.token_gone = true;
        self.dirty = true;
    }

    fn retire(&mut self) {
        let revisions = std::mem::take(&mut self.revisions);
        *self = Self {
            revisions,
            ..Self::default()
        };
    }
}
