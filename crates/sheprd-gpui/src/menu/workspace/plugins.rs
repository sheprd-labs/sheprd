//! The workspace popover's plugin actions: which it lists, and running one on
//! the workspace the popover was opened on.

use crate::{
    SheprdWindow,
    i18n::Localize,
    menu::WorkspaceMenuAction,
    plugin_actions::{Scope, Target},
};
use gpui::{prelude::*, *};
use sheprd_session::Keeper;

impl SheprdWindow {
    /// Takes the open workspace popover's plugin rows from its host's last
    /// answer. A workspace the app keeps runs on Herdr 2, which has none.
    pub(crate) fn sync_workspace_plugin_rows(&mut self) {
        let Some(target) = &self.menu.target else {
            return;
        };
        let rows = if target.keeper == Keeper::Herdr1 {
            self.plugin_rows(Scope::Workspace)
        } else {
            Vec::new()
        };
        let Some(target) = &mut self.menu.target else {
            return;
        };
        if target.plugins == rows {
            return;
        }
        // The row the keyboard selected may now be another action.
        if matches!(
            self.menu.workspace_selected,
            Some(WorkspaceMenuAction::Plugin(_))
        ) {
            self.menu.workspace_selected = None;
        }
        target.plugins = rows;
    }

    /// Runs plugin row `index` on the popover's workspace. A workspace gone
    /// since the popover opened closes it and says so.
    pub(in crate::menu) fn activate_workspace_plugin(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = &self.menu.target else {
            return;
        };
        let Some(row) = target.plugins.get(index).cloned() else {
            return;
        };
        let current = self.menu_target_current()
            && self.live.status.is_connected()
            && self.live.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.boot_id == target.boot_id
                    && snapshot
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.workspace_id == target.id)
            });
        let workspace = target.id.clone();
        let started = if current {
            self.run_plugin_action(&row, Target::Workspace(workspace), None, window, cx)
        } else {
            Err(crate::Error::StaleWorkspace)
        };
        if let Err(error) = started {
            self.dismiss_menu(window, cx);
            self.show_flash(crate::window::Flash::warning(error.localize()), cx);
        }
    }
}
