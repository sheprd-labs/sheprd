//! The edits that turn what a core's document last held into what the app
//! holds now: workspaces kept, renamed, moved, or forgotten, and tabs
//! opened, changed, or closed.

use crate::browser::shared::View;
use crate::core_terminals::panes::{Axis, Panes};
use sheprd_backend::TerminalId;
use sheprd_session::spaces::{Edit, Tab, TabEdit};

/// The edits from `base`, the document as last read, to `local`, the
/// store's view of the same scope now. None when nothing changed.
pub(crate) fn edits(base: &View, local: &View) -> Vec<Edit> {
    let mut edits = Vec::new();
    for workspace in &local.workspaces {
        let Some(was) = base.workspace(&workspace.id) else {
            edits.push(Edit::KeepWorkspace(workspace.clone()));
            continue;
        };
        if workspace.custom && (workspace.label != was.label || !was.custom) {
            edits.push(Edit::RenameWorkspace {
                id: workspace.id.clone(),
                label: workspace.label.clone(),
            });
        }
        // Other clients read a workspace's checkout, branch, and pull
        // request from its folder, and show the name the document holds:
        // both follow the workspace here, a name only while no one gave one.
        let renamed = !workspace.custom && !was.custom && workspace.label != was.label;
        if !workspace.cwd.is_empty() && (workspace.cwd != was.cwd || renamed) {
            edits.push(Edit::FollowWorkspace {
                id: workspace.id.clone(),
                cwd: workspace.cwd.clone(),
                label: workspace.label.clone(),
            });
        }
    }
    // The order the document would hold after these edits; the store's
    // when they agree, else every workspace moves to its place.
    let mut order: Vec<&str> = (base.workspaces.iter())
        .map(|workspace| workspace.id.as_str())
        .filter(|id| local.workspace(id).is_some())
        .collect();
    order.extend(
        (local.workspaces.iter())
            .filter(|workspace| base.workspace(&workspace.id).is_none())
            .map(|workspace| workspace.id.as_str()),
    );
    let wanted: Vec<&str> = (local.workspaces.iter())
        .map(|workspace| workspace.id.as_str())
        .collect();
    if order != wanted {
        for (at, workspace) in local.workspaces.iter().enumerate().rev() {
            edits.push(Edit::MoveWorkspace {
                id: workspace.id.clone(),
                before: local.workspaces.get(at + 1).map(|next| next.id.clone()),
            });
        }
    }
    // Terminals a tab lets go of are let go first, by closing it or its
    // pane, so a pane moved to another tab is free when it lands there.
    for tab in &base.tabs {
        if local.tab(&tab.id).is_none() {
            edits.push(Edit::CloseTab { id: tab.id.clone() });
        }
    }
    let mut landing = Vec::new();
    for tab in &local.tabs {
        let Some(was) = base.tab(&tab.id) else {
            landing.push(Edit::OpenTab(tab.clone()));
            continue;
        };
        let (early, late) = tab_edits(was, tab);
        let id = &tab.id;
        let edit = |edit| Edit::Tab {
            id: id.clone(),
            edit,
        };
        edits.extend(early.into_iter().map(edit));
        landing.extend(late.into_iter().map(edit));
    }
    edits.extend(landing);
    for workspace in &base.workspaces {
        if local.workspace(&workspace.id).is_none() {
            edits.push(Edit::ForgetWorkspace {
                id: workspace.id.clone(),
            });
        }
    }
    edits
}

/// The changes from `was` to `now` of one tab, each touching only its own
/// fields, so a change another client made to the same tab stays, split as
/// [`pane_edits`] splits them. Its program's title changes as it runs and
/// is not written.
fn tab_edits(was: &Tab, now: &Tab) -> (Vec<TabEdit>, Vec<TabEdit>) {
    let mut edits = Vec::new();
    if was.workspace_id != now.workspace_id {
        edits.push(TabEdit::Move {
            workspace_id: now.workspace_id.clone(),
        });
    }
    let (early, late) = pane_edits(&was.panes, &now.panes);
    edits.extend(late);
    if was.name != now.name {
        edits.push(TabEdit::Rename {
            name: now.name.clone(),
        });
    }
    let renamed = (now.names.keys())
        .chain(was.names.keys())
        .filter(|terminal| was.names.get(*terminal) != now.names.get(*terminal))
        .collect::<std::collections::BTreeSet<_>>();
    for terminal in renamed {
        let Ok(id) = TerminalId::new(terminal.clone()) else {
            continue;
        };
        // A pane closed here takes its name with it.
        if now.panes.contains(&id) {
            edits.push(TabEdit::RenamePane {
                terminal: id,
                name: now.names.get(terminal).cloned(),
            });
        }
    }
    if was.zoomed != now.zoomed {
        edits.push(TabEdit::Zoom {
            zoomed: now.zoomed.clone(),
        });
    }
    (early, edits)
}

/// The pane changes from `was` to `now`, as the edits that keep another
/// client's change to the same tab: each pane closed and each pane opened
/// (a split), then the layout over the same terminals for what moved
/// (resizing, swapping). Answers what goes before other tabs land panes,
/// so a pane moved away is free, and what goes after: a last pane closes
/// only once its replacement is there.
pub(crate) fn pane_edits(was: &Panes, now: &Panes) -> (Vec<TabEdit>, Vec<TabEdit>) {
    let (mut early, mut late) = (Vec::new(), Vec::new());
    if was == now {
        return (early, late);
    }
    let mut panes = was.clone();
    let mut last = Vec::new();
    for terminal in was.terminals() {
        if now.contains(terminal) {
            continue;
        }
        if panes.count() > 1 && panes.remove(terminal) {
            early.push(TabEdit::RemovePane {
                terminal: terminal.clone(),
            });
        } else {
            last.push(terminal.clone());
        }
    }
    let opened: Vec<&TerminalId> = (now.terminals().into_iter())
        .filter(|terminal| !panes.contains(terminal))
        .collect();
    for (at_end, new) in opened
        .iter()
        .enumerate()
        .map(|(n, new)| (n + 1 == opened.len(), *new))
    {
        // Where the layout has it when that is one split away; else beside
        // the pane before it, and the layout below puts it in place.
        let exact = (at_end && last.is_empty())
            .then(|| {
                panes.terminals().into_iter().find_map(|at| {
                    [Axis::Row, Axis::Column].into_iter().find_map(|axis| {
                        let mut tried = panes.clone();
                        tried.split(at, axis, new.clone()).ok()?;
                        (tried == *now).then(|| (at.clone(), axis))
                    })
                })
            })
            .flatten();
        let (at, axis) = exact.unwrap_or_else(|| {
            let before = (now.terminals().into_iter())
                .take_while(|terminal| *terminal != new)
                .filter(|terminal| panes.contains(terminal))
                .last()
                .unwrap_or_else(|| panes.first())
                .clone();
            (before, Axis::Row)
        });
        if panes.split(&at, axis, new.clone()).is_ok() {
            late.push(TabEdit::Split {
                at,
                axis,
                new: new.clone(),
            });
        }
    }
    for terminal in last {
        if panes.remove(&terminal) {
            late.push(TabEdit::RemovePane { terminal });
        }
    }
    if panes != *now {
        late.push(TabEdit::SetPanes { panes: now.clone() });
    }
    (early, late)
}
