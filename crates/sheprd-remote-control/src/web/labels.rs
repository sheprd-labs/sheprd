//! Host-local agent and worktree labels. Control requests serialize edits on
//! their worker threads. Home subscribers only borrow the published snapshot.
use crate::{
    app,
    control::{
        ErrorCode, Response,
        labels::{Edit, Target},
    },
    hub::{self, Hub, Layout, Workspace},
};
use sheprd_client::device_tags::{MAX_TAG_CHARS, MAX_TAGS, same};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};
use tokio::sync::watch;

pub(crate) type Snapshot = BTreeMap<String, Vec<String>>;
const MAX_TARGETS: usize = 4096;

pub(crate) struct Labels {
    file: Option<PathBuf>,
    current: watch::Sender<Arc<Snapshot>>,
    editing: Mutex<()>,
}

impl Labels {
    pub(crate) fn load(dir: &Path) -> Self {
        let file = dir.join("agent-labels.json");
        let saved = fs::File::open(&file).and_then(|reader| {
            // Bound startup even if a different program damaged the private file.
            if reader.metadata()?.len() > 64 * 1024 * 1024 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "labels file too large",
                ));
            }
            Ok(serde_json::from_reader(io::BufReader::new(reader))?)
        });
        let (file, labels) = match saved {
            Ok(labels) => (Some(file), labels),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (Some(file), Snapshot::new()),
            Err(error) => {
                tracing::warn!(%error, "Could not load agent labels; keeping the file for recovery");
                (None, Snapshot::new())
            }
        };
        Self {
            file,
            current: watch::Sender::new(Arc::new(labels)),
            editing: Mutex::new(()),
        }
    }

    pub(crate) fn current(&self) -> Arc<Snapshot> {
        self.current.borrow().clone()
    }
    pub(crate) fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.current.subscribe()
    }

    pub(crate) fn answer(&self, hub: &Hub, target: Target, worktree: bool, edit: Edit) -> Response {
        let Some(file) = &self.file else {
            return refused("Could not read the saved agent labels");
        };
        let layout = hub.layout();
        let Some(key) = resolve(hub, &layout, target, worktree) else {
            return refused("No matching agent or worktree on this computer");
        };
        let _guard = self.editing.lock().unwrap_or_else(PoisonError::into_inner);
        let current = self.current();
        let saved = current.get(&key).cloned().unwrap_or_default();
        let mut tags = saved.clone();
        if let Err(error) = change(&mut tags, edit) {
            return refused(&error.to_string());
        }
        if tags != saved {
            let mut next = (*current).clone();
            if tags.is_empty() {
                next.remove(&key);
            } else {
                if next.len() >= MAX_TARGETS && !next.contains_key(&key) {
                    // Drop expired agent identities only. A closed worktree can reopen.
                    let active = layout
                        .workspaces
                        .iter()
                        .flat_map(|w| {
                            w.chats.iter().map(|c| chat_key(c.endpoint.as_str())).chain(
                                w.terminals
                                    .iter()
                                    .flat_map(|t| t.agents.iter().chain(t.agent.iter()))
                                    .map(terminal_key),
                            )
                        })
                        .collect::<std::collections::HashSet<_>>();
                    next.retain(|k, _| k.starts_with("worktree:") || active.contains(k));
                    if next.len() >= MAX_TARGETS {
                        return refused("Too many labeled targets on this computer");
                    }
                }
                next.insert(key, tags.clone());
            }
            if let Err(error) = app::edited::save(file, &next) {
                return Response::Error {
                    code: ErrorCode::Unsaved,
                    message: error.to_string(),
                };
            }
            self.current.send_replace(Arc::new(next));
        }
        Response::Labels { tags }
    }
}

fn refused(message: &str) -> Response {
    Response::Error {
        code: ErrorCode::Other,
        message: message.to_owned(),
    }
}

pub(crate) fn chat_key(endpoint: &str) -> String {
    format!("chat:{endpoint}")
}

pub(crate) fn worktree_key(w: &Workspace) -> Option<String> {
    let cwd = w.checkout_folder()?;
    let distro = hub::wsl_distribution(&w.id).unwrap_or_default();
    (cwd.len() <= 4096 && !cwd.chars().any(char::is_control))
        .then(|| format!("worktree:{distro}:{cwd}"))
}

fn resolve(hub: &Hub, layout: &Layout, target: Target, worktree: bool) -> Option<String> {
    let pane = match target {
        Target::Workspace(id) => return worktree_key(layout.workspace(&id)?),
        Target::Chat(endpoint) => layout
            .chats()
            .find(|c| c.endpoint.as_str() == endpoint)?
            .pane
            .clone(),
        Target::Pane(pane) => pane,
        Target::LocalPane { pane, socket } => layout
            .workspaces
            .iter()
            .filter(|w| {
                hub.workspace_target(w.session.as_deref())
                    .socket_path()
                    .ok()
                    .as_deref()
                    == Some(Path::new(&socket))
            })
            .flat_map(|w| {
                w.chats.iter().map(|c| &c.pane).chain(
                    w.terminals
                        .iter()
                        .flat_map(|t| t.panes.iter().map(|p| &p.pane)),
                )
            })
            .find(|id| local_pane(id) == pane)?
            .clone(),
    };
    if let Some((workspace, chat)) = layout.chat(&pane) {
        return if worktree {
            worktree_key(workspace)
        } else {
            Some(chat_key(chat.endpoint.as_str()))
        };
    }
    let (workspace, terminal) = layout.terminal(&pane)?;
    if worktree {
        return worktree_key(workspace);
    }
    terminal
        .agents
        .iter()
        .chain(terminal.agent.iter())
        .find(|a| a.pane == pane)
        .map(terminal_key)
}

pub(crate) fn terminal_key(agent: &hub::TerminalAgent) -> String {
    format!("pane:{}:{}", agent.boot, agent.pane)
}

fn local_pane(pane: &str) -> &str {
    pane.strip_prefix("herdr:")
        .and_then(|id| id.split_once(':'))
        .map_or(pane, |(_, id)| id)
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("A label must contain 1 to {MAX_TAG_CHARS} printable characters")]
    Invalid,
    #[error("This target already has {MAX_TAGS} labels")]
    Full,
    #[error("No such label on this target")]
    Missing,
}

fn label(text: &str) -> Result<String, Error> {
    if text.chars().any(char::is_control) {
        return Err(Error::Invalid);
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.chars().count() > MAX_TAG_CHARS {
        return Err(Error::Invalid);
    }
    Ok(text)
}

fn change(tags: &mut Vec<String>, edit: Edit) -> Result<(), Error> {
    match edit {
        Edit::List => {}
        Edit::Add(text) => {
            let tag = label(&text)?;
            if !tags.iter().any(|t| same(t, &tag)) {
                if tags.len() >= MAX_TAGS {
                    return Err(Error::Full);
                }
                tags.push(tag);
            }
        }
        Edit::Remove(text) => {
            let tag = label(&text)?;
            tags.retain(|t| !same(t, &tag));
        }
        Edit::Rename { old, new } => {
            let old = label(&old)?;
            let new = label(&new)?;
            let index = tags
                .iter()
                .position(|t| same(t, &old))
                .ok_or(Error::Missing)?;
            if tags
                .iter()
                .enumerate()
                .any(|(i, t)| i != index && same(t, &new))
            {
                tags.remove(index);
            } else {
                tags[index] = new;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_reject_invalid_edits_and_merge_a_rename_into_an_existing_tag() {
        let mut tags = vec!["Review".into(), "mobile".into()];
        assert!(change(&mut tags, Edit::Add(" MOBILE ".into())).is_ok());
        assert!(
            change(
                &mut tags,
                Edit::Rename {
                    old: "review".into(),
                    new: "Mobile".into()
                }
            )
            .is_ok()
        );
        assert_eq!(tags, ["mobile"]);
        for text in [" ", "one\ntwo", &"a".repeat(MAX_TAG_CHARS + 1)] {
            assert!(matches!(
                change(&mut tags, Edit::Add(text.into())),
                Err(Error::Invalid)
            ));
        }
        tags = (0..MAX_TAGS).map(|i| i.to_string()).collect();
        assert!(matches!(
            change(&mut tags, Edit::Add("extra".into())),
            Err(Error::Full)
        ));
        assert_eq!(tags.len(), MAX_TAGS);
    }
}
