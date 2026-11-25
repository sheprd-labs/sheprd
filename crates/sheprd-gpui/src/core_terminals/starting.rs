//! Places completed core starts in their captured workspace and editor group.
use crate::{
    SheprdWindow,
    browser::{Location, Store},
    core_terminals::{Purpose, panes::Panes},
    i18n::fl,
    window::Flash,
};
use gpui::{Context, Window};

impl SheprdWindow {
    pub(super) fn place_started_core_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, _, started)) = &self.core_terminals.starting else {
            return;
        };
        let outcome = match started.try_recv() {
            Ok(outcome) => outcome.take(),
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.core_terminals.starting = None;
                return;
            }
        };
        let Some((purpose, host, _)) = self.core_terminals.starting.take() else {
            return;
        };
        let info = match outcome {
            // Known at once, so a chat it labels attaches without waiting
            // for the next list.
            Ok(info) => match self.core_terminals.infos_of_mut(&host) {
                Some(infos) => infos.insert(info),
                None => info,
            },
            Err(error) => {
                let reason = error.to_string();
                self.show_flash(
                    Flash::warning(fl!("core-terminals-start-failed", reason = reason)),
                    cx,
                );
                return;
            }
        };
        match purpose {
            // The kept workspace it was for closed while it started: end it
            // rather than leave a shell no tab shows.
            Purpose::Tab {
                scope, workspace, ..
            } if self.kept_workspace_gone(&scope, &workspace, cx) => {
                self.end_core_terminal(&host, info.id);
            }
            Purpose::Tab {
                focus,
                scope,
                workspace,
                group,
            } => {
                let terminal = info.id.clone();
                let location = Location::core_terminal(Panes::single(info.id), false);
                let Some(id) = Store::update(cx, |store| {
                    store.open(scope, &workspace, Some(location), None)
                }) else {
                    self.end_core_terminal(&host, terminal);
                    self.show_flash(Flash::warning(fl!("core-terminals-too-many-tabs")), cx);
                    return;
                };
                self.sync_core_views(cx);
                if focus {
                    self.show_browser_tab_in(group, id, window, cx);
                }
            }
            Purpose::Split { tab, at, axis } => {
                self.place_split(tab, &at, axis, (host, info.id), window, cx)
            }
        }
        cx.notify();
    }
}
