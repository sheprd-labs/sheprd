//! Plugin action tests, and the stand-in host the menus' tests run against.
#![allow(clippy::unwrap_used)]

use super::*;

// Host scripts need /bin/sh; plugin actions are not offered on other clients.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod bridge_script;
mod catalog_rows;
mod host_context;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod hosts;

/// The catalog Herdr's `plugin.action.list` and `plugin.list` answers make
/// on a macOS host.
pub(crate) fn catalog(actions: serde_json::Value, plugins: serde_json::Value) -> catalog::Catalog {
    let listed: Vec<catalog::Listed> = serde_json::from_value(actions).unwrap();
    let installed: Vec<catalog::Installed> = serde_json::from_value(plugins).unwrap();
    catalog::Catalog::new(Some(catalog::Platform::Macos), listed, &installed)
}

/// A host the menus' tests reach instead of a daemon: it answers each list
/// with the catalog a test gave it, records each action it is asked to run,
/// and answers with the refusal a test gave it, if any. GPUI runs background
/// work on the test's own thread, so the state is per test thread. Only the
/// menus' tests use it, and they run where host scripts do.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod fake {
    pub(crate) use super::catalog;
    use super::{Bridge, Catalogs, Error, Result, bridge::Invocation, catalog::Catalog};
    use crate::{SheprdWindow, teleport::Host};
    use std::{cell::RefCell, path::PathBuf, sync::atomic::AtomicBool};

    #[derive(Default)]
    struct State {
        /// The next list's answer; `None` fails the read.
        catalog: Option<Catalog>,
        fetches: usize,
        invocations: Vec<Invocation>,
        /// Herdr's refusal of the next actions, as `(code, message)`.
        refusal: Option<(String, String)>,
        /// Runs actions through the real bridge against a stand-in `herdr`
        /// in this home instead of answering them here.
        script_home: Option<PathBuf>,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::default();
    }

    fn fetch(_: &Host, _: &AtomicBool) -> Result<Catalog> {
        STATE.with_borrow_mut(|state| {
            state.fetches += 1;
            state.catalog.clone().ok_or(Error::Unanswered)
        })
    }

    fn invoke(host: &Host, invocation: &Invocation, cancelled: &AtomicBool) -> Result<()> {
        let (home, refusal) = STATE.with_borrow_mut(|state| {
            state.invocations.push(invocation.clone());
            (state.script_home.clone(), state.refusal.clone())
        });
        if let Some(home) = home {
            let mut host = host.clone();
            host.env = vec![
                ("HOME".into(), home.to_string_lossy().into_owned()),
                ("PATH".into(), "/usr/bin:/bin".into()),
            ];
            return super::bridge::invoke(&host, invocation, cancelled);
        }
        match refusal {
            Some((code, message)) => Err(Error::Refused { code, message }),
            None => Ok(()),
        }
    }

    /// Points `view`'s endpoint at this machine, which a script can reach,
    /// and its plugin actions at the stand-in host, with a fresh state.
    pub(crate) fn install(view: &mut SheprdWindow) {
        STATE.with_borrow_mut(|state| *state = State::default());
        view.endpoints[view.selected_endpoint].connection.target =
            sheprd_client::ConnectTarget::Local;
        view.plugin_actions = Catalogs::with_bridge(Bridge { fetch, invoke });
    }

    /// Two plugins, each with an action for every menu: Hunk reviews a
    /// pane's changes and takes a selection, Notes appends to a note.
    pub(crate) fn hunk_and_notes() -> Catalog {
        catalog(
            serde_json::json!([
                {"plugin_id": "hunk", "action_id": "review-pane", "title": "Review changes in Hunk", "contexts": ["pane"]},
                {"plugin_id": "hunk", "action_id": "review-tab", "title": "Review tab in Hunk", "contexts": ["tab"]},
                {"plugin_id": "hunk", "action_id": "review-workspace", "title": "Review workspace in Hunk", "contexts": ["workspace"]},
                {"plugin_id": "hunk", "action_id": "send-selection", "title": "Send selection to Hunk", "contexts": ["selection"]},
                {"plugin_id": "notes", "action_id": "append", "title": "Append to notes", "contexts": ["pane", "selection"]},
                {"plugin_id": "notes", "action_id": "tab-summary", "title": "Summarize tab", "contexts": ["tab"]},
                {"plugin_id": "notes", "action_id": "workspace-log", "title": "Open workspace log", "contexts": ["workspace"]},
            ]),
            serde_json::json!([
                {"plugin_id": "hunk", "name": "Hunk", "enabled": true},
                {"plugin_id": "notes", "name": "Notes", "enabled": true},
            ]),
        )
    }

    pub(crate) fn answer(catalog: Catalog) {
        STATE.with_borrow_mut(|state| state.catalog = Some(catalog));
    }

    /// Fails every list read from now on, as a host without Herdr does.
    pub(crate) fn fail_reads() {
        STATE.with_borrow_mut(|state| state.catalog = None);
    }

    pub(crate) fn refuse(code: &str, message: &str) {
        STATE.with_borrow_mut(|state| state.refusal = Some((code.into(), message.into())));
    }

    pub(crate) fn run_through_script(home: PathBuf) {
        STATE.with_borrow_mut(|state| state.script_home = Some(home));
    }

    pub(crate) fn fetches() -> usize {
        STATE.with_borrow(|state| state.fetches)
    }

    /// The actions run so far, which a call clears.
    pub(crate) fn take_invocations() -> Vec<Invocation> {
        STATE.with_borrow_mut(|state| std::mem::take(&mut state.invocations))
    }

    /// The plugin and action of each action run so far, which a call clears.
    pub(crate) fn take_run() -> Vec<String> {
        take_invocations()
            .into_iter()
            .map(|invocation| format!("{}/{}", invocation.plugin_id, invocation.action_id))
            .collect()
    }
}
