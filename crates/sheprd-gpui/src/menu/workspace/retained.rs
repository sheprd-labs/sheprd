use super::{Label, Page, WorkspaceMenuAction, WorkspaceTarget};
use crate::SheprdWindow;
use gpui::{prelude::*, *};

impl SheprdWindow {
    /// The reveal action and its platform-specific wording, for local paths.
    pub(super) fn reveal_item(&self) -> Option<(WorkspaceMenuAction, Label)> {
        if self.selected_is_remote() {
            return None;
        }
        #[cfg(target_os = "macos")]
        {
            Some((
                WorkspaceMenuAction::RevealInFileManager,
                Label::REVEAL_IN_FINDER,
            ))
        }
        #[cfg(target_os = "linux")]
        {
            Some((
                WorkspaceMenuAction::RevealInFileManager,
                Label::REVEAL_IN_FILE_MANAGER,
            ))
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }

    /// Opens the minimal menu for a project row retained after tab closure.
    pub(crate) fn open_closed_project_menu(
        &mut self,
        endpoint: &str,
        id: &str,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            return;
        }
        let project = self
            .endpoints
            .iter()
            .find(|host| host.id == endpoint)
            .and_then(|host| {
                let scope = crate::browser::scope(host);
                cx.try_global::<crate::browser::Store>()
                    .and_then(|store| store.closed_project(&scope, id))
                    .cloned()
            });
        let Some(project) = project else {
            return;
        };
        if !self.select_endpoint(endpoint, cx) || self.reveal_item().is_none() {
            return;
        }
        self.hover = None;
        self.hover_menu = None;
        self.menu.reset();
        self.menu.endpoint_target = (
            self.selection_epoch,
            self.endpoints[self.selected_endpoint].generation,
        );
        self.menu.target = Some(WorkspaceTarget::retained(
            project.id,
            project.label,
            project.cwd,
            project.branch,
        ));
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Workspace);
        self.menu.opening_right_click = true;
        self.discard_composition(cx);
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    /// The minimal menu for a project row retained after tab closure.
    pub(super) fn retained_workspace_items(&self) -> Vec<(WorkspaceMenuAction, Label)> {
        let mut items: Vec<(WorkspaceMenuAction, Label)> = self.reveal_item().into_iter().collect();
        items.push((WorkspaceMenuAction::RemoveProject, Label::REMOVE_PROJECT));
        items
    }

    /// Reveals a workspace's folder in Finder or the system file manager.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(super) fn reveal_workspace_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = &self.menu.target {
            crate::settings_window::reveal_path(std::path::PathBuf::from(&target.cwd), cx);
        }
        self.dismiss_menu(window, cx);
    }
}
