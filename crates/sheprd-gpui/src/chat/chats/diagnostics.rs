//! Explain attachment decisions once per change, including panes that never
//! make it into the bounded set and therefore never produce a link error.
//! Each line says how long the focused pane has waited since focus reached
//! it, so `native_chat_available` records how long switching to a chat took.

use super::Chats;
use crate::chat::{MAX_CHATS, Route, Target};
use sheprd_chat_protocol::TOKEN;
use sheprd_client::protocol::ClientShellSnapshot;
use std::time::Instant;

pub(super) struct Decision {
    host_index: usize,
    pane: String,
    reason: &'static str,
    /// When focus reached the pane.
    since: Instant,
    transport: &'static str,
}

impl Route {
    pub(super) fn transport_label(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ssh(host) if sheprd_client::relay_host::is_relay_target(host) => "relay_ssh",
            Self::Ssh(_) => "ssh",
            Self::Remote(_) => "remote_connector",
        }
    }
}

impl Chats {
    /// Logs why `focused`, a core tab's pane or the daemon's, does or does
    /// not show its chat, when that changed.
    pub(in crate::chat) fn log_focused_attachment(
        &mut self,
        focused: Option<&str>,
        known: &[Target],
        snapshot: Option<&ClientShellSnapshot>,
        host_index: usize,
        enabled: bool,
        route: Option<&Route>,
    ) {
        let Some(pane) = focused else {
            self.diagnostic = None;
            return;
        };
        let core = pane.starts_with("core");
        // A core tab's chat has the route of its own host.
        let route = (self.pane(pane).map(|chat| &chat.target))
            .or_else(|| known.iter().find(|target| target.pane_id == pane))
            .map(|target| &target.route)
            .or(route.filter(|_| !core));
        let reason = if !enabled {
            "acp_chats_disabled"
        } else if route.is_none() && !core {
            "no_chat_route"
        } else if let Some(chat) = self.pane(pane) {
            if chat.available() {
                if self.raw.contains(&chat.target) {
                    "terminal_selected"
                } else {
                    "native_chat_available"
                }
            } else if chat.permanent {
                "attachment_stopped"
            } else if chat.retry_at.is_some() {
                "attachment_retry_pending"
            } else {
                "awaiting_transcript"
            }
        } else {
            untracked_reason(snapshot, pane, known)
        };
        let transport = route.map_or("none", Route::transport_label);
        let last = self.diagnostic.as_ref();
        let since = last
            .filter(|last| last.host_index == host_index && last.pane == pane)
            .map_or_else(Instant::now, |last| last.since);
        if last.is_some_and(|last| {
            last.host_index == host_index
                && last.pane == pane
                && last.reason == reason
                && last.transport == transport
        }) {
            return;
        }
        tracing::info!(
            pane,
            host_index,
            reason,
            transport,
            waited_ms = crate::chat::millis_since(since),
            targets = self.panes.len(),
            resting = self.resting.len(),
            limit = route.map_or(MAX_CHATS, Route::max_chats),
            "Focused pane chat attachment"
        );
        self.diagnostic = Some(Decision {
            host_index,
            pane: pane.to_owned(),
            reason,
            since,
            transport,
        });
    }
}

/// Why `pane` has no chat attached. A known target without an attached pane
/// was excluded by the window's bound.
fn untracked_reason(
    snapshot: Option<&ClientShellSnapshot>,
    pane: &str,
    known: &[Target],
) -> &'static str {
    if known.iter().any(|target| target.pane_id == pane) {
        return "chat_limit_reached";
    }
    if pane.starts_with("core") {
        return "no_core_chat";
    }
    let Some(snapshot) = snapshot else {
        return "no_agent_report";
    };
    let mut reason = "no_agent_report";
    for agent in snapshot.agents.iter().filter(|agent| agent.pane_id == pane) {
        if agent.tokens.iter().any(|(name, _)| name == TOKEN) {
            reason = "invalid_acp_endpoint";
        } else if reason == "no_agent_report" {
            reason = "no_acp_endpoint";
        }
    }
    reason
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::discovery;

    #[test]
    fn paired_ssh_routes_have_a_distinct_transport_label() {
        let relay = sheprd_client::relay_host::host_name("0123456789abcdef");
        assert_eq!(
            Route::Ssh(format!("user@{relay}")).transport_label(),
            "relay_ssh"
        );
        assert_eq!(Route::Ssh("user@studio".into()).transport_label(), "ssh");
        assert_eq!(Route::Local.transport_label(), "local");
    }

    #[test]
    fn excluded_acp_pane_is_distinguished_from_missing_metadata() {
        let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
        let template = snapshot.agents[0].clone();
        snapshot.agents = (0..=MAX_CHATS)
            .map(|n| {
                let mut agent = template.clone();
                agent.pane_id = format!("p{n}");
                agent.tokens = vec![(TOKEN.into(), "0".repeat(32))];
                agent
            })
            .collect();
        let excluded = format!("p{MAX_CHATS}");
        let known = discovery::targets(&snapshot, Some(&Route::Local));
        let targets = discovery::bounded(known.clone(), |_| 2);
        assert!(targets.iter().all(|target| target.pane_id != excluded));
        assert_eq!(
            untracked_reason(Some(&snapshot), &excluded, &known),
            "chat_limit_reached"
        );

        let visible = discovery::bounded(known, |pane| u8::from(pane != excluded));
        assert!(visible.iter().any(|target| target.pane_id == excluded));

        snapshot.agents[MAX_CHATS].tokens[0].1.clear();
        assert_eq!(
            untracked_reason(Some(&snapshot), &excluded, &[]),
            "invalid_acp_endpoint"
        );
        snapshot.agents[MAX_CHATS].tokens.clear();
        assert_eq!(
            untracked_reason(Some(&snapshot), &excluded, &[]),
            "no_acp_endpoint"
        );
        assert_eq!(
            untracked_reason(Some(&snapshot), "absent", &[]),
            "no_agent_report"
        );
        assert_eq!(untracked_reason(None, "core:t1", &[]), "no_core_chat");
    }
}
