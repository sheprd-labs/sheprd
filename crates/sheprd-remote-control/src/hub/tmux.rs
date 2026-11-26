//! Managed local tmux chats expose the same private ACP endpoints as Herdr
//! chats. Inventory discovery never attaches to a pane or revives a process.
use super::{ChatRow, Layout, Workspace, core, layout::MAX_CHATS, sessions};
use sheprd_backend::{AgentStatus, Backend, RunState, TerminalInfo};
use sheprd_client::ConnectTarget;
use sheprd_session::SavedTabs;
use std::{collections::HashSet, path::Path, sync::Arc, time::Duration};
use tokio::sync::watch;

const UNLISTED: &str = "tmux:chats";

fn preferred() -> bool {
    super::wake::settings_path()
        .is_some_and(|p| super::wake::chosen(&p) == sheprd_backend::TerminalBackend::Tmux)
}

fn scrollback() -> usize {
    super::wake::settings_path().map_or(super::wake::DEFAULT_SCROLLBACK, |p| {
        super::wake::scrollback(&p)
    })
}

impl super::Hub {
    pub(super) async fn wants_tmux_chat(&self, workspace: &str) -> bool {
        self.search_program
            && (workspace == UNLISTED
                || tokio::task::spawn_blocking(preferred)
                    .await
                    .unwrap_or(false))
    }
}

pub(super) fn open_chat(
    agent: sheprd_chat_protocol::Agent,
    launcher: &super::Launcher,
    workspace: &str,
    cwd: &str,
    folder: &str,
) -> super::Result<String> {
    let workspace = if workspace == UNLISTED {
        sheprd_session::DEFAULT_WORKSPACE
    } else {
        workspace
    };
    let endpoint = sheprd_chat_protocol::EndpointId::random().to_string();
    let mut labels = core::place_labels(Some(super::place(workspace, folder)))
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect::<std::collections::BTreeMap<_, _>>();
    labels.insert(sheprd_chat_protocol::TOKEN.to_owned(), endpoint.clone());
    let spawn = sheprd_backend::Spawn {
        argv: launcher.argv(agent),
        cwd: (!cwd.is_empty()).then(|| cwd.into()),
        env: vec![(sheprd_chat_protocol::ENDPOINT_ENV.to_owned(), endpoint)],
        labels,
        ..Default::default()
    };
    let info = sheprd_tmux::Tmux::new(scrollback()).spawn(&spawn)?;
    Ok(crate::control::favorites::tmux_pane(info.id.as_str()))
}

pub(super) fn with_chats(
    base: watch::Receiver<Arc<Layout>>,
    target: &ConnectTarget,
    state_dir: &Path,
    sessions: Arc<sessions::Sessions>,
) -> watch::Receiver<Arc<Layout>> {
    let scope = target
        .socket_path()
        .ok()
        .map(|p| format!("local:{}", p.display()));
    let tabs = core::tabs_path(state_dir);
    let (sender, source) = watch::channel(Arc::new(Layout::default()));
    let (merged, result) = watch::channel(Arc::new(Layout::default()));
    tokio::spawn(core::merge_sources(merged, base, source));
    // Recovery is independent of inventory polling, so an unavailable agent
    // cannot delay a phone opening a different live chat.
    let lifetime = sender.clone();
    tokio::task::spawn_blocking(move || recover_chats(&lifetime));
    tokio::spawn(async move {
        let backend = sheprd_tmux::Tmux::new(0);
        loop {
            let backend = backend.clone();
            let named = sessions.local_scopes();
            let tabs = tabs.clone();
            let scope = scope.clone();
            let Ok(next) = tokio::task::spawn_blocking(move || {
                let Ok(records) = backend.terminals() else {
                    return Layout::default();
                };
                let selected = preferred();
                if records.is_empty() && !selected {
                    return Layout::default();
                }
                let saved = tabs.as_deref().and_then(core::read_tabs);
                let saved = saved.as_ref().zip(scope.as_deref());
                let mut layout = project(records, saved, &named);
                if selected && let Some(home) = sheprd_client::local_path::home_dir() {
                    list_home(&mut layout, saved, &home);
                }
                layout
            })
            .await
            else {
                return;
            };
            let interval = if next.connected { 2 } else { 10 };
            sender.send_if_modified(|current| {
                if **current == next {
                    return false;
                }
                *current = Arc::new(next);
                true
            });
            tokio::select! {
                () = sender.closed() => return,
                () = tokio::time::sleep(Duration::from_secs(interval)) => {}
            }
        }
    });
    result
}

/// Scan once per service start. Each pane is claimed on its owning host
/// before exec, and only if its private chat record belongs to an older boot.
fn recover_chats(lifetime: &watch::Sender<Arc<Layout>>) {
    let Some(dir) = sheprd_chat_protocol::restore::restore_dir_with(sheprd_client::login_env::var)
    else {
        return;
    };
    let Ok(records) = std::fs::read_dir(dir.join("tmux")) else {
        return;
    };
    let backend = sheprd_tmux::Tmux::new(scrollback());
    let mut remaining = MAX_CHATS;
    for entry in records.flatten() {
        if remaining == 0 || lifetime.is_closed() {
            break;
        }
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "chat") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|stem| sheprd_backend::TerminalId::new(stem).ok())
            .filter(|id| id.is_tmux())
        else {
            continue;
        };
        match backend.recover_chat(&id) {
            Ok(Some(_)) => remaining -= 1,
            Ok(None) | Err(sheprd_tmux::Error::NameTaken) => {}
            Err(error) => tracing::warn!(%id, %error, "Could not resume a managed tmux chat"),
        }
    }
}

/// An empty tmux host still has somewhere to start its first chat.
fn list_home(layout: &mut Layout, saved: Option<(&SavedTabs, &str)>, home: &Path) {
    let id = sheprd_session::DEFAULT_WORKSPACE;
    if layout.workspace(id).is_none() {
        let kept = saved.and_then(|(tabs, scope)| tabs.workspaces(scope).find(|w| w.id == id));
        layout.workspaces.push(Workspace {
            id: id.into(),
            cwd: kept.map_or_else(|| home.to_string_lossy().into_owned(), |w| w.cwd.clone()),
            label: kept.map_or_else(|| "Home".into(), |w| w.label.clone()),
            branch: None,
            worktree: None,
            session: None,
            chats: vec![],
            terminals: vec![],
        });
    }
    layout.connected = true;
    layout.boot = "tmux".into();
}

fn project(
    records: Vec<TerminalInfo>,
    saved: Option<(&SavedTabs, &str)>,
    named: &std::collections::BTreeMap<String, String>,
) -> Layout {
    let (tabs, scope) = saved.unzip();
    let mut placements = core::tmux_placements(tabs, scope, named);
    let mut layout = Layout::default();
    let mut endpoints = HashSet::new();
    for info in records {
        if info.state != RunState::Running || !info.id.is_tmux() {
            continue;
        }
        let Some(endpoint) = sheprd_core_client::convert::chat_endpoint(&info) else {
            continue;
        };
        if !endpoints.insert(endpoint.clone()) {
            continue;
        }
        let (workspace, name) = placements.remove(&info.id).unzip();
        let workspace = workspace
            .or_else(|| {
                sheprd_session::labelled_workspace(&info.labels)
                    .map(|w| sessions::join_id(w.session, w.id))
            })
            .unwrap_or_else(|| UNLISTED.to_owned());
        let row = match layout.workspaces.iter().position(|w| w.id == workspace) {
            Some(i) => &mut layout.workspaces[i],
            None => {
                let kept = saved
                    .and_then(|(tabs, scope)| tabs.workspaces(scope).find(|w| w.id == workspace));
                layout.workspaces.push_mut(Workspace {
                    label: kept.map_or_else(|| "tmux".to_owned(), |w| w.label.clone()),
                    cwd: kept
                        .map(|w| w.cwd.clone())
                        .or_else(|| {
                            info.cwd
                                .as_ref()
                                .and_then(|p| p.to_str())
                                .map(str::to_owned)
                        })
                        .unwrap_or_default(),
                    id: workspace,
                    branch: None,
                    worktree: None,
                    session: None,
                    chats: Vec::new(),
                    terminals: Vec::new(),
                })
            }
        };
        let pane = crate::control::favorites::tmux_pane(info.id.as_str());
        let title = name
            .flatten()
            .or(info.title)
            .or(info.name)
            .unwrap_or_default();
        row.chats.push(ChatRow {
            tab: pane.clone(),
            pane,
            endpoint,
            agent: info.reporter,
            named: !title.trim().is_empty(),
            title,
            status: info.agent,
        });
        layout.working += usize::from(info.agent == AgentStatus::Working);
        if endpoints.len() >= MAX_CHATS {
            break;
        }
    }
    if !layout.workspaces.is_empty() {
        layout.connected = true;
        layout.boot = "tmux".to_owned();
    }
    layout
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests;
