//! The "Share this session" modal dialog: lets an owner configure read-only
//! sharing defaults (history boundary, tool opt-in, expiry, trusted relay),
//! inspect the exact host-side guest preview, mint a one-time HTTPS fragment
//! invitation link, and stop active shares. All daemon calls run on the
//! background executor via `Supervisor::client`.
// i18n: migrated

mod render;

use super::supervisor::{Client, Health, Lease, Supervisor};
use crate::{SheprdWindow, menu::Page};
use gpui::{ClipboardItem, Context, KeyDownEvent, Modifiers, ScrollHandle, Task, Window};
use sheprd_remote_control::control::{
    RelayState, Request, Response,
    sharing::{ShareCreated, ShareHistory, SharePreview, ShareSummary},
};
use uuid::Uuid;

pub(super) const LIFETIMES: &[(u64, &str)] = &[
    (15 * 60, "remote-share-expiry-15m"),
    (30 * 60, "remote-share-expiry-30m"),
    (60 * 60, "remote-share-expiry-1h"),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PreviewState {
    Loading,
    Ready(SharePreview),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BusyState {
    Idle,
    Creating,
    Stopping,
}

/// State of the open Share this session dialog. Dropping it cancels any
/// in-flight preview or action tasks without touching active shares on the daemon.
pub(crate) struct ShareDialog {
    pub(super) pane: String,
    pub(super) agent: String,
    pub(super) enabled: bool,
    pub(super) history: ShareHistory,
    pub(super) include_tools: bool,
    pub(super) lifetime_secs: u64,
    pub(super) relays: Vec<String>,
    pub(super) selected_relay: usize,
    pub(super) preview: PreviewState,
    pub(super) active: Vec<ShareSummary>,
    pub(super) created: Option<ShareCreated>,
    pub(super) copied: bool,
    pub(super) busy: BusyState,
    pub(super) error: Option<String>,
    pub(super) preview_scroll: ScrollHandle,
    client: Option<Client>,
    _lease: Option<Lease>,
    _list_task: Option<Task<()>>,
    _preview_task: Option<Task<()>>,
    _action_task: Option<Task<()>>,
}

impl ShareDialog {
    pub(super) fn selected_relay(&self) -> Option<&str> {
        self.relays.get(self.selected_relay).map(String::as_str)
    }

    pub(crate) fn close(&mut self) {
        self._lease = None;
        self._list_task = None;
        self._preview_task = None;
        self._action_task = None;
    }
}

impl Drop for ShareDialog {
    fn drop(&mut self) {
        self.close();
    }
}

fn listening_relays(health: &Health) -> Vec<String> {
    let Health::Running(status) = health else {
        return Vec::new();
    };
    let mut listening = Vec::new();
    let mut configured = Vec::new();
    for (endpoint, relay) in &status.relays {
        if !endpoint.starts_with("https://") {
            continue;
        }
        if matches!(relay.state, RelayState::Listening { .. }) {
            listening.push(endpoint.clone());
        } else {
            configured.push(endpoint.clone());
        }
    }
    listening.extend(configured);
    listening
}

impl SheprdWindow {
    /// Opens the Share this session dialog for `pane_id`, unless another modal
    /// page already holds the window.
    pub(crate) fn open_share_session(
        &mut self,
        pane_id: String,
        agent: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return false;
        }
        let enabled = self.config.remote.enabled;
        let client = Supervisor::client(cx);
        let relays = listening_relays(&Supervisor::health(cx));
        self.menu.remote_share = Some(ShareDialog {
            pane: pane_id,
            agent,
            enabled,
            // D5 deliberate defaults: FromNow, tool text excluded, 1 hour.
            history: ShareHistory::FromNow,
            include_tools: false,
            lifetime_secs: 60 * 60,
            relays,
            selected_relay: 0,
            preview: PreviewState::Loading,
            active: Vec::new(),
            created: None,
            copied: false,
            busy: BusyState::Idle,
            error: None,
            preview_scroll: ScrollHandle::new(),
            client,
            _lease: Supervisor::lease(cx),
            _list_task: None,
            _preview_task: None,
            _action_task: None,
        });
        self.menu.page = Some(Page::ShareSession);
        if enabled {
            self.refresh_share_list(cx);
            self.refresh_share_preview(cx);
        }
        cx.notify();
        true
    }

    pub(crate) fn share_session_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        if event.keystroke.modifiers == Modifiers::default()
            && event.keystroke.key.as_str() == "escape"
        {
            self.dismiss_menu(window, cx);
        }
    }

    pub(super) fn set_share_history(&mut self, history: ShareHistory, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        if dialog.history == history {
            return;
        }
        dialog.history = history;
        dialog.error = None;
        self.refresh_share_preview(cx);
        cx.notify();
    }

    pub(super) fn set_share_tools(&mut self, include_tools: bool, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        if dialog.include_tools == include_tools {
            return;
        }
        dialog.include_tools = include_tools;
        dialog.error = None;
        self.refresh_share_preview(cx);
        cx.notify();
    }

    pub(super) fn set_share_lifetime(&mut self, lifetime_secs: u64, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        dialog.lifetime_secs = lifetime_secs;
        cx.notify();
    }

    pub(super) fn cycle_share_relay(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        if dialog.relays.len() > 1 {
            dialog.selected_relay = (dialog.selected_relay + 1) % dialog.relays.len();
            cx.notify();
        }
    }

    pub(super) fn refresh_share_list(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        let Some(client) = dialog.client.clone() else {
            return;
        };
        let pane = dialog.pane.clone();
        dialog._list_task = Some(cx.spawn(async move |this, cx| {
            let result = client.request(Request::ShareList).await;
            let _ = this.update(cx, |view, cx| {
                let Some(dialog) = view.menu.remote_share.as_mut() else {
                    return;
                };
                if dialog.pane != pane {
                    return;
                }
                if let Ok(Response::Shares { shares }) = result {
                    dialog.active = shares
                        .into_iter()
                        .filter(|share| share.pane == pane)
                        .collect();
                    let active = !dialog.active.is_empty();
                    if dialog.relays.is_empty() {
                        dialog.relays = listening_relays(&Supervisor::health(cx));
                    }
                    Supervisor::set_pane_shared(cx, &pane, active);
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn refresh_share_preview(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        let Some(client) = dialog.client.clone() else {
            dialog.preview = PreviewState::Failed(crate::i18n::fl!("remote-share-unavailable"));
            return;
        };
        let pane = dialog.pane.clone();
        let history = dialog.history;
        let include_tools = dialog.include_tools;
        dialog.preview = PreviewState::Loading;
        dialog._preview_task = Some(cx.spawn(async move |this, cx| {
            let result = client
                .request(Request::SharePreview {
                    pane: pane.clone(),
                    history,
                    include_tools,
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let Some(dialog) = view.menu.remote_share.as_mut() else {
                    return;
                };
                if dialog.pane != pane
                    || dialog.history != history
                    || dialog.include_tools != include_tools
                {
                    return;
                }
                dialog.preview = match result {
                    Ok(Response::SharePreview(preview)) => PreviewState::Ready(preview),
                    Ok(_) => PreviewState::Failed(crate::remote::Error::Unexpected.to_string()),
                    Err(error) => PreviewState::Failed(error.to_string()),
                };
                cx.notify();
            });
        }));
    }

    pub(super) fn create_share_link(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        if dialog.busy != BusyState::Idle {
            return;
        }
        let Some(client) = dialog.client.clone() else {
            dialog.error = Some(crate::i18n::fl!("remote-share-unavailable"));
            cx.notify();
            return;
        };
        let pane = dialog.pane.clone();
        let history = dialog.history;
        let include_tools = dialog.include_tools;
        let lifetime_secs = dialog.lifetime_secs;
        let relay = dialog.selected_relay().map(str::to_owned);
        dialog.busy = BusyState::Creating;
        dialog.error = None;
        dialog.copied = false;
        dialog._action_task = Some(cx.spawn(async move |this, cx| {
            let result = client
                .request(Request::ShareCreate {
                    pane: pane.clone(),
                    history,
                    include_tools,
                    lifetime_secs,
                    relay,
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let Some(dialog) = view.menu.remote_share.as_mut() else {
                    return;
                };
                if dialog.pane != pane {
                    return;
                }
                dialog.busy = BusyState::Idle;
                match result {
                    Ok(Response::ShareCreated(created)) => {
                        dialog.active.push(created.share.clone());
                        dialog.created = Some(created);
                        Supervisor::set_pane_shared(cx, &pane, true);
                    }
                    Ok(_) => {
                        dialog.error = Some(crate::remote::Error::Unexpected.to_string());
                    }
                    Err(error) => {
                        dialog.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn copy_share_link(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        if let Some(created) = &dialog.created {
            cx.write_to_clipboard(ClipboardItem::new_string(created.link.expose().to_owned()));
            dialog.copied = true;
            cx.notify();
        }
    }

    pub(super) fn stop_share(&mut self, share: Uuid, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_share.as_mut() else {
            return;
        };
        if dialog.busy != BusyState::Idle {
            return;
        }
        let Some(client) = dialog.client.clone() else {
            return;
        };
        dialog.busy = BusyState::Stopping;
        dialog.error = None;
        dialog._action_task = Some(cx.spawn(async move |this, cx| {
            let result = client
                .request(Request::ShareRevoke { share, grant: None })
                .await;
            let _ = this.update(cx, |view, cx| {
                let Some(dialog) = view.menu.remote_share.as_mut() else {
                    return;
                };
                dialog.busy = BusyState::Idle;
                match result {
                    Ok(Response::Done) => {
                        dialog.active.retain(|entry| entry.share != share);
                        let pane = dialog.pane.clone();
                        let still_active = !dialog.active.is_empty();
                        if dialog
                            .created
                            .as_ref()
                            .is_some_and(|created| created.share.share == share)
                        {
                            dialog.created = None;
                            dialog.copied = false;
                        }
                        Supervisor::set_pane_shared(cx, &pane, still_active);
                    }
                    Ok(_) => {
                        dialog.error = Some(crate::remote::Error::Unexpected.to_string());
                    }
                    Err(error) => {
                        dialog.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
