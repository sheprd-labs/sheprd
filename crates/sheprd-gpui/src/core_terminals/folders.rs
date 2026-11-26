//! The folders views take from herdr-core panes, as they take a Herdr
//! pane's: code search, the editor, and an editor tab's starting folder ask
//! for the folder the user works in, whichever backend runs its terminal.

use super::CoreTerminals;
use crate::{SheprdWindow, browser::Scope};
use gpui::App;
use std::path::PathBuf;

impl CoreTerminals {
    /// The folder a workspace's herdr-core tabs started in, as Herdr names
    /// a workspace's by its first tab's first pane: where the first pane of
    /// the oldest tab that drives its terminals was first seen, so moving
    /// around in that pane does not move the workspace.
    pub(crate) fn launch_root(&self, scope: &Scope, workspace: &str) -> Option<PathBuf> {
        let (_, tab) = self
            .tabs
            .iter()
            .filter(|(_, shown)| !shown.observe)
            .filter(|(_, shown)| {
                shown
                    .workspace
                    .as_ref()
                    .is_some_and(|(known, id)| known == scope && id == workspace)
            })
            .min_by_key(|(id, _)| **id)?;
        tab.root.clone()
    }
}

impl super::TabPanes {
    /// Keeps the first folder herdr-core reports for the tab's first pane.
    pub(super) fn keep_root(&mut self, infos: &super::infos::Infos) {
        if self.root.is_none() {
            self.root = infos
                .get(self.panes.first())
                .and_then(|info| info.cwd.clone());
        }
    }
}

impl CoreTerminals {
    /// The scope and workspace of `tab`, once the window has shown it.
    pub(crate) fn tab_workspace(&self, tab: crate::browser::TabId) -> Option<&(Scope, String)> {
        self.tabs.get(&tab)?.workspace.as_ref()
    }

    /// Each terminal the window shows, by its core and its address there,
    /// and the workspace it is in, for what names a terminal but not its
    /// workspace: the terminals of its tabs, and the shell shown in place of
    /// Herdr 1, in the workspace that stands in for Herdr's. A saved host's
    /// terminals are named with that host, as the same address there is
    /// another terminal than here.
    ///
    /// Each workspace is named with the daemon whose tabs hold it, and the
    /// shell's with `fallback`, the daemon of the host the window shows.
    pub(crate) fn terminal_workspaces(
        &self,
        fallback: Option<Scope>,
    ) -> Vec<(super::Host, String, crate::listening_ports::Place)> {
        let fallback = self
            .fallback_terminal()
            .zip(fallback)
            .map(|(terminal, scope)| {
                (
                    super::Host::Local,
                    terminal.as_str().to_owned(),
                    (scope, super::WORKSPACE.to_owned()),
                )
            });
        // A terminal opened to watch in another workspace stays in the one
        // whose tab drives it: driving tabs come first, oldest first, and the
        // port lookups keep a terminal's first workspace.
        let mut tabs: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|(id, shown)| Some((shown.observe, *id, shown, shown.workspace.as_ref()?)))
            .collect();
        tabs.sort_unstable_by_key(|(observe, id, ..)| (*observe, *id));
        let mut places = Vec::new();
        let mut fallback = fallback;
        for (observe, _, shown, place) in tabs {
            if observe && let Some(shell) = fallback.take() {
                places.push(shell);
            }
            for terminal in shown.panes.terminals() {
                places.push((
                    shown.host.clone(),
                    terminal.as_str().to_owned(),
                    place.clone(),
                ));
            }
        }
        places.extend(fallback);
        places
    }
}

impl SheprdWindow {
    /// Keeps the last observed folder through terminal closure and app restart.
    pub(crate) fn keep_core_workspace_folders(&self, scope: &Scope, cx: &mut App) {
        let changes: Vec<_> = cx
            .try_global::<crate::browser::Store>()
            .into_iter()
            .flat_map(|store| {
                store.kept_workspaces(scope).filter_map(|workspace| {
                    let cwd = self.core_workspace_cwd(store, scope, &workspace.id)?;
                    (cwd != workspace.cwd).then(|| (workspace.id.clone(), cwd.to_owned()))
                })
            })
            .collect();
        if !changes.is_empty() {
            crate::browser::Store::update(cx, |store| {
                for (id, cwd) in changes {
                    store.follow_kept_folder(scope, &id, &cwd);
                }
            });
        }
    }

    /// A workspace follows its oldest driving tab's first pane, independent
    /// of focus and split groups. The saved folder covers tabs not yet read.
    pub(super) fn core_workspace_cwd<'a>(
        &'a self,
        store: &'a crate::browser::Store,
        scope: &Scope,
        workspace: &str,
    ) -> Option<&'a str> {
        let host = super::Host::of(scope);
        let terminal = store
            .in_workspace(scope, workspace)
            .filter_map(|tab| match &tab.location {
                Some(crate::browser::Location::CoreTerminal {
                    panes,
                    observe: false,
                    ..
                }) => Some((tab.id, panes.first())),
                _ => None,
            })
            .min_by_key(|(id, _)| *id)
            .map(|(_, terminal)| terminal)
            .or_else(|| {
                (host == super::Host::Local && workspace == super::WORKSPACE)
                    .then(|| self.core_terminals.fallback_terminal())
                    .flatten()
            });
        terminal
            .and_then(|terminal| self.core_terminals.listed(&host, terminal))
            .and_then(|info| info.cwd.as_deref())
            .and_then(std::path::Path::to_str)
            .filter(|cwd| !cwd.is_empty())
            .or_else(|| {
                store
                    .kept_workspace(scope, workspace)
                    .map(|saved| saved.cwd.as_str())
            })
            .filter(|cwd| !cwd.is_empty())
    }

    /// The folder of the pane the user works in: the herdr-core pane the
    /// active group shows, else Herdr's focused pane, where its program is
    /// rather than where its shell started.
    pub(crate) fn focused_folder(&self, cx: &App) -> Option<String> {
        if let Some((tab, terminal)) = self.shown_core_pane(cx) {
            return self
                .core_folder(tab, &terminal)
                .and_then(|folder| folder.into_os_string().into_string().ok());
        }
        let snapshot = self.live.snapshot.as_deref()?;
        let id = snapshot.focused_pane_id.as_deref()?;
        let pane = snapshot.panes.iter().find(|pane| pane.pane_id == id)?;
        pane.foreground_cwd.clone().or_else(|| pane.cwd.clone())
    }

    /// The checkout of the herdr-core pane the active group showed at the
    /// last tick, once Git named its branch. `None` when no herdr-core tab
    /// is shown, or its pane is in no checkout or on a detached head.
    pub(crate) fn core_git_input(&self) -> Option<crate::pull_request::Input> {
        let (tab, terminal) = self.core_terminals.shown.as_ref()?;
        self.core_pane_git_input(*tab, terminal)
    }

    pub(crate) fn core_pane_git_input(
        &self,
        tab: crate::browser::TabId,
        terminal: &sheprd_backend::TerminalId,
    ) -> Option<crate::pull_request::Input> {
        let host = self.core_terminals.host_of(tab);
        if host
            != super::Host::of(&crate::browser::scope(
                &self.endpoints[self.selected_endpoint],
            ))
        {
            return None;
        }
        let checkout = self
            .core_terminals
            .info(tab, terminal)?
            .cwd
            .as_ref()?
            .to_str()?
            .to_owned();
        let checkouts = match &host {
            super::Host::Local => &self.core_terminals.checkouts,
            super::Host::Remote(scope) => {
                let remote = self.core_terminals.remote.get(scope)?;
                if remote.target != self.endpoints[self.selected_endpoint].connection.target {
                    return None;
                }
                &remote.checkouts
            }
        };
        let identity = checkouts.get(&checkout)?;
        let branch = identity.branch.clone()?;
        Some(crate::pull_request::Input {
            checkout: Some(checkout),
            repo_key: (host != super::Host::Local).then(|| identity.key.clone()),
            branch,
        })
    }

    /// The folder of the herdr-core pane the active group showed at the
    /// last tick.
    pub(super) fn shown_core_folder(&self) -> Option<String> {
        let (tab, terminal) = self.core_terminals.shown.as_ref()?;
        self.core_terminals
            .info(*tab, terminal)?
            .cwd
            .clone()?
            .into_os_string()
            .into_string()
            .ok()
    }

    /// Whether the active group showed a herdr-core tab at the last tick.
    pub(crate) fn shows_core_tab(&self) -> bool {
        self.core_terminals.shown.is_some()
    }
}

impl SheprdWindow {
    /// Saves the name each workspace the user has not named shows in
    /// `sessions`, which its folder gives it, so the shared document carries
    /// it to other clients. Only once Git answered for that folder: a linked
    /// worktree keeps its saved name, which a name read before would replace
    /// for good. The store changes only when a name did.
    pub(super) fn keep_core_workspace_names(
        &self,
        sessions: &[(Scope, std::sync::Arc<sheprd_session::Session>)],
        cx: &mut App,
    ) {
        let changes: Vec<(&Scope, &str, &str)> = cx
            .try_global::<crate::browser::Store>()
            .into_iter()
            .flat_map(|store| {
                sessions.iter().flat_map(move |(scope, session)| {
                    let checkouts = match super::Host::of(scope) {
                        super::Host::Local => Some(&self.core_terminals.checkouts),
                        super::Host::Remote(scope) => self
                            .core_terminals
                            .remote
                            .get(&scope)
                            .map(|remote| &remote.checkouts),
                    };
                    session.workspaces.iter().filter_map(move |workspace| {
                        let kept = store.kept_workspace(scope, &workspace.id)?;
                        let answered =
                            checkouts.is_some_and(|known| known.answered(&workspace.cwd));
                        (answered
                            && !workspace.custom_label
                            && !kept.custom
                            && kept.label != workspace.label)
                            .then_some((scope, workspace.id.as_str(), workspace.label.as_str()))
                    })
                })
            })
            .collect();
        if !changes.is_empty() {
            crate::browser::Store::update(cx, |store| {
                for (scope, id, label) in changes {
                    store.follow_kept_label(scope, id, label);
                }
            });
        }
    }
}
