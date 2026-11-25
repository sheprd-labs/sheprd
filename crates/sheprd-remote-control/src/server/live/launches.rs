//! Announcements from the desktop's launch path. The control socket accepts
//! them before the child tab opens. The notifier consumes them in turn order.
use super::group::Group;
use crate::push::excerpt;
use sheprd_chat_protocol::{EndpointId, brief::Delegation};
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

/// A tab whose chat never attaches must not hold the card indefinitely.
const STARTUP_MS: u64 = 120_000;
const MAX_WAITING: usize = 256;

#[derive(Default)]
pub(crate) struct Launches(Mutex<HashMap<EndpointId, Launch>>);

struct Launch {
    owner: Delegation,
    label: Option<String>,
    cancelled: bool,
    until_ms: u64,
}

impl Launches {
    pub(crate) fn report(
        &self,
        owner: Delegation,
        child: EndpointId,
        label: Option<String>,
        now: u64,
    ) {
        let mut waiting = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        waiting.retain(|_, launch| launch.until_ms > now);
        let cancelled = label.is_none();
        if let Some(launch) = waiting.get_mut(&child) {
            // A tab can fail before the notifier consumes its announcement.
            // Keep the original name and deliver both transitions together.
            launch.cancelled |= cancelled;
        } else if waiting.len() < MAX_WAITING {
            waiting.insert(
                child,
                Launch {
                    owner,
                    label: label.map(|s| excerpt(&s)),
                    cancelled,
                    until_ms: now.saturating_add(STARTUP_MS),
                },
            );
        }
    }

    pub(super) fn deliver(&self, group: &mut Group, now: u64) -> bool {
        let mut changed = false;
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|child, launch| {
                if launch.until_ms <= now {
                    return false;
                }
                if !group.owns(&launch.owner) {
                    return true;
                }
                if let Some(label) = &launch.label {
                    changed |= group.launch(child, Some(label), launch.until_ms);
                }
                if launch.cancelled {
                    changed |= group.launch(child, None, launch.until_ms);
                }
                false
            });
        changed
    }
}
