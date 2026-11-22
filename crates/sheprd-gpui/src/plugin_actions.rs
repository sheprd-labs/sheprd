//! Plugin actions in the pane, tab, and workspace menus.
//!
//! Herdr plugins declare actions with the contexts they belong to (`pane`,
//! `tab`, `workspace`, `selection`). Each menu lists the actions of its
//! context below its own rows, grouped by plugin, and runs one on the pane,
//! tab, or workspace it was opened on.
//!
//! Each host's actions are read when one of these menus opens, on a
//! background worker; the menu shows the host's last answer at once and
//! takes the new one when it lands, so it never waits on the host. A host
//! that cannot answer leaves the menu with its own rows only.
//!
//! Herdr 2 (herdr-core) has no plugins, so its menus have no such rows.

pub(crate) mod bridge;
pub(crate) mod catalog;
mod context;
mod error;
mod render;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use catalog::{Row, Scope};
pub(crate) use context::Target;
pub use error::{Error, Result};
pub(crate) use render::{Layout, child_index};

use crate::SheprdWindow;
use gpui::{prelude::*, *};
use sheprd_client::scrollback::SelectionReadParams;
use std::{collections::HashMap, sync::atomic::AtomicBool};

/// The last actions each host offered, by [`crate::teleport::Host::identity`]:
/// its route, session, and installation. An endpoint keeps its id when the
/// user picks another session behind it, so the id alone would show one
/// session's actions in another's menus.
///
/// The default reaches no host: it reads no actions and runs none, so a
/// window built without [`Catalogs::with_bridge`], as headless tests build
/// theirs, never touches a real daemon.
#[derive(Debug, Default)]
pub(crate) struct Catalogs {
    by_host: HashMap<String, Entry>,
    bridge: Option<Bridge>,
}

/// How a host is asked for its actions and told to run one. Blocking: both
/// run on a background worker.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bridge {
    pub(crate) fetch: fn(&crate::teleport::Host, &AtomicBool) -> Result<catalog::Catalog>,
    pub(crate) invoke: fn(&crate::teleport::Host, &bridge::Invocation, &AtomicBool) -> Result<()>,
}

impl Bridge {
    /// Herdr's API socket, through `herdr remote-api-bridge` on the host.
    pub(crate) const API: Self = Self {
        fetch: bridge::fetch,
        invoke: bridge::invoke,
    };
}

#[derive(Debug, Default)]
struct Entry {
    catalog: catalog::Catalog,
    /// A read in flight, which another menu opening does not repeat.
    fetching: bool,
}

impl Catalogs {
    /// Catalogs that ask hosts through `bridge`.
    pub(crate) fn with_bridge(bridge: Bridge) -> Self {
        Self {
            by_host: HashMap::new(),
            bridge: Some(bridge),
        }
    }

    fn rows(&self, host: &str, scope: Scope) -> Vec<Row> {
        self.by_host
            .get(host)
            .map(|entry| entry.catalog.rows(scope))
            .unwrap_or_default()
    }
}

impl SheprdWindow {
    /// The host the selected endpoint's session runs on.
    fn plugin_host(&self) -> Result<crate::teleport::Host> {
        bridge::host_for(&self.endpoints[self.selected_endpoint].connection.target)
    }

    /// The rows a menu on the selected session shows for `scope`, from its
    /// host's last answer. A host no script reaches has none.
    pub(crate) fn plugin_rows(&self, scope: Scope) -> Vec<Row> {
        self.plugin_host()
            .map(|host| self.plugin_actions.rows(&host.identity(), scope))
            .unwrap_or_default()
    }

    /// Reads the selected session's actions again in the background. A
    /// host no script reaches has none, so it is not asked.
    pub(crate) fn refresh_plugin_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Bridge { fetch, .. }) = self.plugin_actions.bridge else {
            return;
        };
        let Ok(host) = self.plugin_host() else {
            return;
        };
        let id = host.identity();
        let entry = self.plugin_actions.by_host.entry(id.clone()).or_default();
        if entry.fetching {
            return;
        }
        entry.fetching = true;
        // The CLI is a child process, possibly over SSH: never on this thread.
        let job = cx
            .background_executor()
            .spawn(async move { fetch(&host, &AtomicBool::new(false)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| this.plugin_actions_read(&id, result, cx));
        })
        .detach();
    }

    /// Keeps a host's new answer, and updates the open menu's rows if it
    /// changed them and the menu is still on that host. A failed read keeps
    /// the last answer: the menu's own rows never depend on it, and the
    /// action itself reports what is wrong.
    fn plugin_actions_read(
        &mut self,
        host: &str,
        result: Result<catalog::Catalog>,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.plugin_actions.by_host.get_mut(host) else {
            return;
        };
        entry.fetching = false;
        match result {
            Ok(catalog) if catalog != entry.catalog => entry.catalog = catalog,
            Ok(_) => return,
            Err(error) => {
                tracing::debug!(%error, "Plugin actions not read");
                return;
            }
        }
        if self.plugin_host().ok().map(|now| now.identity()).as_deref() != Some(host) {
            return;
        }
        self.sync_pane_plugin_rows();
        self.sync_tab_plugin_rows();
        self.sync_workspace_plugin_rows();
        cx.notify();
    }

    /// Runs `row`'s action on `target` and closes the menu. Herdr's answer
    /// comes later, from a background worker; a failure then flashes on the
    /// window. Fails at once only when the endpoint has no scriptable host.
    pub(crate) fn run_plugin_action(
        &mut self,
        row: &Row,
        target: Target,
        selection: Option<SelectionReadParams>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> crate::Result<()> {
        let Bridge { invoke, .. } = self.plugin_actions.bridge.ok_or(Error::UnsupportedHost)?;
        let host = self.plugin_host()?;
        let invocation = bridge::Invocation {
            plugin_id: row.plugin_id.clone(),
            action_id: row.action_id.clone(),
            target,
            selection: selection.filter(|_| row.takes_selection),
        };
        let title = row.title.clone();
        self.dismiss_menu(window, cx);
        let job = cx
            .background_executor()
            .spawn(async move { invoke(&host, &invocation, &AtomicBool::new(false)) });
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = job.await {
                let _ = this.update(cx, |this, cx| {
                    use crate::i18n::Localize;
                    let text = crate::i18n::fl!(
                        "plugin-actions-failed",
                        action = title.as_str(),
                        error = error.localize()
                    );
                    this.show_flash(crate::window::Flash::warning(text), cx);
                });
            }
        })
        .detach();
        Ok(())
    }
}
