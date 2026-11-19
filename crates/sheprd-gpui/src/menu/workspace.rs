//! The workspace menu's dialogs and actions: what the window opens for a
//! target (`target.rs`), and the requests it carries to the daemon.

// i18n: migrated
mod dispatch;
mod kept;
mod kept_worktrees;
pub(super) mod popover;
mod render;
mod requests;
mod reuse;
#[cfg(feature = "cloud")]
mod sandbox_images;
mod scripts;
mod target;

pub(super) use kept_worktrees::Ask;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(crate) use kept_worktrees::Landing as KeptLanding;
pub(crate) use kept_worktrees::{KeptDone, remove_kept_checkout_on};
pub(crate) use target::WorkspaceTarget;
pub(super) use target::{
    NewWorktreeUnavailable, Source, is_main_of, known_worktree, main_checkout,
};

use super::{Page, WorkspaceAction, WorkspaceMenuAction, state::Deletion};
use crate::{SheprdWindow, dialog_input::DialogInput};
use gpui::{prelude::*, *};
use popover::Label;
use sheprd_client::{Method, protocol::ClientShellSnapshot};

/// The workspace focused when the new worktree shortcut is pressed, once it
/// is known to have a source for one.
fn new_worktree_source(snapshot: &ClientShellSnapshot) -> Result<String, NewWorktreeUnavailable> {
    let focused = snapshot
        .workspaces
        .iter()
        .find(|w| Some(&w.workspace_id) == snapshot.focused_workspace_id.as_ref())
        .ok_or(NewWorktreeUnavailable::NoWorkspace)?;
    WorkspaceTarget::for_new_worktree(snapshot, focused)?;
    Ok(focused.workspace_id.clone())
}

/// The name a new tab dialog proposes: the next number in its workspace, as
/// Herdr numbers an unnamed tab.
pub(super) fn suggested_tab_name(snapshot: &ClientShellSnapshot, workspace: &str) -> String {
    let count = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == workspace)
        .count();
    (count + 1).to_string()
}

/// The name a new workspace dialog proposes: the folder it opens in. Herdr
/// also consults Git there, which only the daemon's host can do, so leaving
/// this proposal unchanged still lets the daemon choose the real name.
pub(super) fn suggested_workspace_name(cwd: &str) -> String {
    let trimmed = cwd.trim_end_matches('/');
    match trimmed.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ if cwd.is_empty() => "workspace".to_owned(),
        _ => "/".to_owned(),
    }
}

/// The label a creation sends. An empty name or the unchanged proposal sends
/// none, so the daemon names the tab or workspace as Herdr's own prompt does.
pub(super) fn chosen_label<'a>(text: &'a str, suggested: Option<&str>) -> Option<&'a str> {
    let text = text.trim();
    (!text.is_empty() && Some(text) != suggested).then_some(text)
}

impl SheprdWindow {
    /// The reveal action and its platform-specific wording, for local paths.
    fn reveal_item(&self) -> Option<(WorkspaceMenuAction, Label)> {
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

    /// The selected endpoint's workspace a workspace menu is open for. A
    /// hover menu can target a row that is not focused, so the row marks
    /// itself while the pointer is over the menu instead.
    pub(crate) fn workspace_menu_target(&self) -> Option<&str> {
        if self.menu.page != Some(Page::Workspace) {
            return None;
        }
        self.menu.target.as_ref().map(|target| target.id.as_str())
    }

    pub(crate) fn open_workspace_menu(
        &mut self,
        id: &str,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_none() && self.is_kept_workspace(id) {
            self.open_kept_workspace_menu(id, anchor, window, cx);
            return;
        }
        if self.menu.page.is_some() || !self.live.status.is_connected() {
            return;
        }
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        let Some(workspace) = snapshot.workspaces.iter().find(|w| w.workspace_id == id) else {
            return;
        };
        self.hover = None;
        self.hover_menu = None;
        self.menu.reset();
        self.menu.endpoint_target = (
            self.selection_epoch,
            self.endpoints[self.selected_endpoint].generation,
        );
        let muted = self.muted_on_selected(crate::notification_mutes::Scope::Workspace, id, cx);
        self.menu.target = Some(WorkspaceTarget {
            muted,
            ..WorkspaceTarget::new(snapshot, workspace)
        });
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Workspace);
        self.refresh_workspace_pr();
        self.discard_composition(cx);
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    /// Opens the new worktree dialog for the focused workspace, as its menu's
    /// "New worktree" row would. When there is no source for one, a flash says
    /// why rather than the shortcut doing nothing visible.
    pub(crate) fn open_new_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(kept) = self.kept_in_front().map(str::to_owned) {
            self.open_workspace_menu(&kept, Point::default(), window, cx);
            let offered = self.workspace_items().iter().any(|(action, _)| {
                *action == WorkspaceMenuAction::Dialog(WorkspaceAction::NewWorktree)
            });
            if !offered {
                self.dismiss_menu(window, cx);
                self.show_flash(
                    crate::window::Flash::warning(NewWorktreeUnavailable::NotGit.message()),
                    cx,
                );
                return;
            }
            self.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            return;
        }
        let source = match &self.live.snapshot {
            Some(snapshot) if self.live.status.is_connected() => new_worktree_source(snapshot),
            _ => Err(NewWorktreeUnavailable::Disconnected),
        };
        let id = match source {
            Ok(id) => id,
            Err(reason) => {
                self.show_flash(crate::window::Flash::warning(reason.message()), cx);
                return;
            }
        };
        self.open_workspace_menu(&id, Point::default(), window, cx);
        if self.menu.page == Some(Page::Workspace) {
            self.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
        }
    }

    /// Opens the focused workspace's rename or close dialog, as its menu's
    /// row would. Closing confirms here as it does from the menu.
    pub(crate) fn open_focused_workspace_dialog(
        &mut self,
        action: WorkspaceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The workspace in front, Herdr 1's or one the app keeps.
        let Some((_, id)) = self.browser_key() else {
            return;
        };
        self.open_workspace_dialog_for(&id, action, Point::default(), window, cx);
    }

    /// Opens `id`'s dialog for `action`, as its menu's row would, so the
    /// dialog checks and targets the workspace exactly as the menu does.
    pub(crate) fn open_workspace_dialog_for(
        &mut self,
        id: &str,
        action: WorkspaceAction,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_menu(id, anchor, window, cx);
        if self.menu.page == Some(Page::Workspace) {
            self.open_workspace_dialog(action, window, cx);
        }
    }

    /// Asks for a name before `command` creates a tab or workspace, when the
    /// local Herdr config's `ui.prompt_new_tab_name` or
    /// `ui.prompt_new_workspace_name` says to. Returns whether the dialog
    /// opened; otherwise the caller creates at once.
    pub(crate) fn open_name_prompt(
        &mut self,
        command: crate::controls::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::controls::Command;
        let prompts = self
            .settings
            .shared
            .as_ref()
            .map(|shared| shared.name_prompts)
            .unwrap_or_default();
        let action = match command {
            Command::Tab if prompts.tab => WorkspaceAction::NewTab,
            Command::Workspace if prompts.workspace => WorkspaceAction::NewWorkspace,
            _ => return false,
        };
        // Only where creating at once would be allowed, so the prompt never
        // offers what the command itself would refuse.
        if self.activation_deadline.is_some()
            || !self.endpoints[self.selected_endpoint].surface_requested()
        {
            return false;
        }
        let Some(id) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_workspace_id.clone())
        else {
            return false;
        };
        self.open_workspace_menu(&id, Point::default(), window, cx);
        if self.menu.page != Some(Page::Workspace) {
            return false;
        }
        self.open_workspace_dialog(action, window, cx);
        true
    }

    /// The actions this workspace offers, in the order the popover shows them
    /// and arrow keys walk them: the header's mute toggle and reveal action,
    /// the tile grid row by row (work, then moving or leaving), the plain
    /// rows, and the destructive action last.
    pub(super) fn workspace_items(&self) -> Vec<(WorkspaceMenuAction, Label)> {
        use WorkspaceMenuAction::Dialog;
        let Some(target) = &self.menu.target else {
            return vec![];
        };
        if target.retained {
            return self.reveal_item().into_iter().collect();
        }
        if target.keeper == sheprd_session::Keeper::App {
            // Scripts run on the shown host, so only for a workspace it
            // keeps: remote ones through the host's POSIX shell, including
            // WSL from a Windows client, and local ones where this computer
            // has a POSIX shell.
            let scripts = self.is_kept_workspace(&target.id)
                && (self.selected_is_remote()
                    || cfg!(any(target_os = "linux", target_os = "macos")));
            let mut items = kept::items(
                target,
                scripts,
                self.note_checkout().is_some(),
                self.checkpoint_checkout().is_some(),
                self.fan_out_item(),
            );
            if let Some(item) = self.reveal_item() {
                items.insert(1, item);
            }
            items.extend(self.teleport_items());
            return items;
        }
        let mut items = vec![if target.muted {
            (WorkspaceMenuAction::Unmute, Label::UNMUTE)
        } else {
            (WorkspaceMenuAction::Mute, Label::MUTE)
        }];
        if let Some(item) = self.reveal_item() {
            items.push(item);
        }
        if target.can_create() || self.linked_new_worktree_target().is_some() {
            items.push((Dialog(WorkspaceAction::NewWorktree), Label::NEW_WORKTREE));
        }
        if let Some(label) = self.fan_out_item() {
            items.push((WorkspaceMenuAction::FanOut, label));
        }
        items.push((Dialog(WorkspaceAction::Rename), Label::RENAME));
        items.extend(self.teleport_items());
        items.push((Dialog(WorkspaceAction::Close), target.close_label()));
        if self.checkpoint_checkout().is_some() {
            items.push((WorkspaceMenuAction::Checkpoints, Label::CHECKPOINTS));
        }
        if self.note_checkout().is_some() {
            items.push((Dialog(WorkspaceAction::Note), Label::NOTE));
        }
        if target.worktree.is_some() && self.orchestrator_host() {
            items.push((WorkspaceMenuAction::Orchestrator, Label::ORCHESTRATOR));
        }
        if target.can_create() {
            items.push((Dialog(WorkspaceAction::OpenWorktree), Label::OPEN_WORKTREE));
        }
        #[cfg(feature = "cloud")]
        if target.worktree.is_some() && !self.sandbox_image_providers().is_empty() {
            items.push((WorkspaceMenuAction::SandboxImages, Label::SANDBOX_IMAGES));
        }
        // Whether the checkout defines scripts is only known once its file is
        // read, so every Git checkout offers them and an absent one says so.
        if target.worktree.is_some() {
            use crate::worktree_scripts::ScriptKind;
            items.push((
                WorkspaceMenuAction::Script(ScriptKind::Run),
                Label::RUN_SCRIPT,
            ));
            if target.can_delete() {
                items.push((
                    WorkspaceMenuAction::Script(ScriptKind::Setup),
                    Label::RUN_SETUP_SCRIPT,
                ));
            }
        }
        // Only a workspace that heads a group of checkouts can fold anything.
        if let Some(key) = target.group_key() {
            items.push(if self.collapsed_repos_for_selection().contains(key) {
                (WorkspaceMenuAction::Expand, Label::EXPAND_GROUP)
            } else {
                (WorkspaceMenuAction::Collapse, Label::COLLAPSE_GROUP)
            });
        }
        if target.can_delete() {
            items.push((
                Dialog(WorkspaceAction::DeleteWorktree),
                Label::DELETE_WORKTREE,
            ));
        }
        items
    }

    /// The collapsed set the sidebar paints for the selected endpoint.
    pub(super) fn collapsed_repos_for_selection(&self) -> &std::collections::HashSet<String> {
        if self.selected_endpoint == 0 {
            &self.collapsed_repos
        } else {
            &self.endpoints[self.selected_endpoint].collapsed_repos
        }
    }

    /// The collapsed set the sidebar paints for the selected endpoint.
    pub(super) fn collapsed_repos_mut(&mut self) -> &mut std::collections::HashSet<String> {
        if self.selected_endpoint == 0 {
            &mut self.collapsed_repos
        } else {
            &mut self.endpoints[self.selected_endpoint].collapsed_repos
        }
    }

    pub(super) fn toggle_selected_group(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self
            .menu
            .target
            .as_ref()
            .and_then(|target| target.group_key())
            .map(str::to_owned)
        else {
            return;
        };
        let collapsed = self.collapsed_repos_mut();
        if !collapsed.remove(&key) {
            collapsed.insert(key);
        }
        self.menu.reset();
        cx.notify();
    }

    /// The main-checkout target a linked checkout's menu creates through.
    pub(super) fn linked_new_worktree_target(&self) -> Option<WorkspaceTarget> {
        let target = self
            .menu
            .target
            .as_ref()
            .filter(|target| target.can_delete())?;
        let snapshot = self.live.snapshot.as_ref()?;
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|w| w.workspace_id == target.id && snapshot.boot_id == target.boot_id)?;
        WorkspaceTarget::for_new_worktree(snapshot, workspace).ok()
    }

    pub(super) fn open_workspace_dialog(
        &mut self,
        action: WorkspaceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Every listing and request of the dialog goes through the main
        // checkout; only the base remembers the linked checkout.
        // A linked checkout opens one only when its main checkout's shell has
        // wandered off; see `WorktreeShortcut::target`.
        if matches!(
            action,
            WorkspaceAction::NewWorktree | WorkspaceAction::OpenWorktree
        ) && let Some(target) = self.linked_new_worktree_target()
        {
            self.menu.target = Some(target);
        }
        let note = (action == WorkspaceAction::Note).then(|| self.note_draft(cx));
        let Some(target) = &self.menu.target else {
            return;
        };
        let workspace = self.live.snapshot.as_ref().and_then(|snapshot| {
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|w| w.workspace_id == target.id && snapshot.boot_id == target.boot_id)?;
            Some((snapshot, workspace))
        });
        // Proposed selected, as Herdr does, so typing replaces it.
        self.menu.suggested_name = match (action, workspace) {
            (WorkspaceAction::NewTab, Some((snapshot, _))) => {
                Some(suggested_tab_name(snapshot, &target.id))
            }
            (WorkspaceAction::NewWorkspace, Some((_, workspace))) => {
                Some(suggested_workspace_name(&workspace.new_workspace_cwd))
            }
            _ => None,
        };
        self.menu.input = match action {
            WorkspaceAction::Rename => Some(DialogInput::new(target.label.clone())),
            WorkspaceAction::NewTab | WorkspaceAction::NewWorkspace => Some(DialogInput::new(
                self.menu.suggested_name.clone().unwrap_or_default(),
            )),
            // Propose the daemon's own branch shape, selected so typing replaces it.
            WorkspaceAction::NewWorktree => {
                Some(DialogInput::branch(crate::worktree::proposed_branch()))
            }
            WorkspaceAction::Note => note.map(DialogInput::new),
            WorkspaceAction::Close
            | WorkspaceAction::DeleteWorktree
            | WorkspaceAction::OpenWorktree => None,
        };
        self.menu.page = Some(Page::Dialog(action));
        // A kept workspace's dialog makes its checkouts here with Git, and
        // another host's through that host's Herdr, as Herdr 1's does.
        self.menu.dispatch = matches!(
            action,
            WorkspaceAction::NewWorktree | WorkspaceAction::NewWorkspace
        )
        .then(|| self.menu.dispatch_repo())
        .flatten()
        .and_then(|repo| {
            crate::dispatch::Picker::opened(
                &self.endpoints[self.selected_endpoint].id,
                self.dispatch_candidates(&repo, cx),
                self.dispatch_sandboxes(),
            )
        });
        self.menu.pr.clear();
        self.menu.pr_connection = None;
        self.menu.error = None;
        if action == WorkspaceAction::OpenWorktree {
            self.open_existing_worktrees(window, cx);
            return;
        }
        if action == WorkspaceAction::NewWorktree {
            self.menu.landing = self.open_landing(cx);
            self.open_worktree_source(window, cx);
            cx.notify();
            return;
        }
        if action == WorkspaceAction::Close
            && target.keeper == sheprd_session::Keeper::Herdr1
            && let Some(snapshot) = &self.live.snapshot
        {
            self.menu.close_check = Some(super::workspace_close::CloseCheck::start(
                snapshot,
                target,
                self.selected_endpoint == 0 && self.live.local_daemon_peer,
                cx,
            ));
        }
        if action == WorkspaceAction::DeleteWorktree && target.keeper == sheprd_session::Keeper::App
        {
            self.open_kept_deletion(cx);
        } else if action == WorkspaceAction::DeleteWorktree {
            let result = self.endpoints[self.selected_endpoint]
                .connection
                .request_dialog(
                    &target.boot_id,
                    Method::WorktreeList,
                    serde_json::json!({"workspace_id": target.id, "trust_repository": false}),
                );
            self.menu.deletion = Some(Deletion::new(
                result.as_ref().ok().cloned(),
                self.removal.as_ref().is_some_and(|removal| {
                    removal.force
                        && removal.workspace == target.id
                        && removal.boot_id == target.boot_id
                }),
            ));
            self.menu.error = result.err().map(|error| error.to_string());
        }
        cx.notify();
    }

    pub(super) fn workspace_menu_actions(&self) -> Vec<WorkspaceMenuAction> {
        let mut actions: Vec<_> = self
            .workspace_items()
            .into_iter()
            .map(|(action, _)| action)
            .collect();
        if self.pr_profile().is_some() && self.menu.pr.value.is_some() {
            actions.push(WorkspaceMenuAction::PullRequest);
        }
        actions
    }

    pub(super) fn activate_workspace_menu(
        &mut self,
        action: WorkspaceMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            WorkspaceMenuAction::Dialog(action) => self.open_workspace_dialog(action, window, cx),
            WorkspaceMenuAction::Collapse | WorkspaceMenuAction::Expand => {
                self.toggle_selected_group(cx)
            }
            WorkspaceMenuAction::Mute | WorkspaceMenuAction::Unmute => {
                if let Some(target) = &self.menu.target {
                    let (id, boot) = (target.id.clone(), target.boot_id.clone());
                    self.set_muted_on_selected(
                        crate::notification_mutes::Scope::Workspace,
                        &id,
                        &boot,
                        action == WorkspaceMenuAction::Mute,
                        cx,
                    );
                }
                self.dismiss_menu(window, cx);
            }
            WorkspaceMenuAction::PullRequest => self.open_workspace_pr(cx),
            WorkspaceMenuAction::Teleport => self.open_teleport(window, cx),
            WorkspaceMenuAction::GoToTeleported => self.go_to_teleported(window, cx),
            WorkspaceMenuAction::TeleportBack => self.teleport_back(window, cx),
            WorkspaceMenuAction::ClearTeleported => self.clear_teleport_mark(window, cx),
            WorkspaceMenuAction::Checkpoints => self.open_checkpoints(window, cx),
            WorkspaceMenuAction::FanOut => self.open_fan_out(window, cx),
            WorkspaceMenuAction::Script(kind) => self.run_workspace_script(kind, window, cx),
            #[cfg(feature = "cloud")]
            WorkspaceMenuAction::SandboxImages => self.build_workspace_sandbox_images(window, cx),
            WorkspaceMenuAction::Orchestrator => {
                if let Some(target) = &self.menu.target {
                    let workspace = target.id.clone();
                    let folder = self.workspace_folder(&workspace);
                    self.open_orchestrator_in(workspace, folder, window, cx);
                }
            }
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            WorkspaceMenuAction::RevealInFileManager => {
                if let Some(target) = &self.menu.target {
                    let folder = std::path::PathBuf::from(&target.cwd);
                    // A project's folder may have been removed since its row
                    // was drawn; launching a file manager at it would only fail.
                    if folder.is_dir() {
                        crate::settings_window::reveal_path(folder, cx);
                    } else {
                        tracing::warn!("Cannot reveal {}: no such folder", folder.display());
                    }
                }
                self.dismiss_menu(window, cx);
            }
        }
    }
}
