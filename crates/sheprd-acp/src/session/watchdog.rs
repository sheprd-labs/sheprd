//! Guards Antigravity turns against lingering background tasks that keep
//! `session/prompt` open indefinitely after the model finishes replying.
//!
//! In ACP v1 a turn ends only when the server returns the `session/prompt`
//! response. When `agy acp` has an unclosed background `run_command` task, it
//! holds `session/prompt` open even after the final assistant reply until the
//! task exits or `session/cancel` terminates background tasks. This watchdog
//! warns once after prolonged silence following an assistant reply and
//! optionally cancels the turn after a longer ceiling without dropping any
//! queued follow-up prompts.

use std::time::{Duration, Instant};

/// Silence after an assistant reply before posting an informational hint.
pub(crate) const WARN_AFTER: Duration = Duration::from_secs(10 * 60);

/// Default silence after an assistant reply before cancelling the turn.
pub(crate) const DEFAULT_CANCEL_AFTER: Duration = Duration::from_secs(60 * 60);

/// Optional environment override for the auto-cancel silence ceiling in
/// seconds; `0` disables auto-cancel while keeping the silence hint.
const CANCEL_AFTER_ENV: &str = "SHEPRD_ANTIGRAVITY_IDLE_TURN_SECS";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Action {
    Warn(Duration),
    Cancel(Duration),
}

#[derive(Debug)]
pub(super) struct Watchdog {
    warn_after: Duration,
    cancel_after: Option<Duration>,
    last_activity: Option<Instant>,
    warned: bool,
    cancelled: bool,
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::new(cancel_after_from_env())
    }
}

impl Watchdog {
    pub(super) fn new(cancel_after: Option<Duration>) -> Self {
        Self {
            warn_after: WARN_AFTER,
            cancel_after,
            last_activity: None,
            warned: false,
            cancelled: false,
        }
    }

    #[cfg(test)]
    pub(super) fn set_cancel_after(&mut self, cancel_after: Option<Duration>) {
        self.cancel_after = cancel_after;
    }

    pub(super) fn start(&mut self, now: Instant) {
        self.last_activity = Some(now);
        self.warned = false;
        self.cancelled = false;
    }

    pub(super) fn touch(&mut self, now: Instant) {
        if self.last_activity.is_some() {
            self.last_activity = Some(now);
            self.warned = false;
            self.cancelled = false;
        }
    }

    pub(super) fn clear(&mut self) {
        self.last_activity = None;
        self.warned = false;
        self.cancelled = false;
    }

    pub(super) fn next_deadline(&self, eligible: bool) -> Option<Instant> {
        if !eligible {
            return None;
        }
        let last = self.last_activity?;
        if !self.warned {
            let next = match self.cancel_after {
                Some(cancel) if cancel <= self.warn_after => cancel,
                _ => self.warn_after,
            };
            return Some(last + next);
        }
        if !self.cancelled {
            return self.cancel_after.map(|cancel| last + cancel);
        }
        None
    }

    pub(super) fn poll(&mut self, now: Instant, eligible: bool) -> Option<Action> {
        if !eligible {
            return None;
        }
        let last = self.last_activity?;
        let elapsed = now.saturating_duration_since(last);
        if !self.cancelled
            && let Some(cancel) = self.cancel_after
            && elapsed >= cancel
        {
            self.cancelled = true;
            self.warned = true;
            return Some(Action::Cancel(cancel));
        }
        if !self.warned && elapsed >= self.warn_after {
            self.warned = true;
            return Some(Action::Warn(self.warn_after));
        }
        None
    }
}

fn cancel_after_from_env() -> Option<Duration> {
    let raw = std::env::var(CANCEL_AFTER_ENV).ok();
    cancel_after_from(raw.as_deref())
}

pub(super) fn cancel_after_from(raw: Option<&str>) -> Option<Duration> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Some(DEFAULT_CANCEL_AFTER),
        Some(value) => match value.parse::<u64>() {
            Ok(0) => None,
            Ok(seconds) => Some(Duration::from_secs(seconds)),
            Err(_) => Some(DEFAULT_CANCEL_AFTER),
        },
    }
}

/// Human-readable duration for notices (e.g. `10 minutes`, `1 hour`).
pub(super) fn describe_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 3600 && secs.is_multiple_of(3600) {
        let hours = secs / 3600;
        if hours == 1 {
            return "1 hour".to_owned();
        }
        return format!("{hours} hours");
    }
    if secs >= 60 && secs.is_multiple_of(60) {
        let mins = secs / 60;
        if mins == 1 {
            return "1 minute".to_owned();
        }
        return format!("{mins} minutes");
    }
    if secs == 1 {
        "1 second".to_owned()
    } else {
        format!("{secs} seconds")
    }
}
