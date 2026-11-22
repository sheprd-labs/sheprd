//! Core owns terminal records; the desktop owns tab and split membership.

use super::{kept::Kept, pane_id};
use crate::hub::{Layout, Terminal, TerminalPane, Workspace, layout::TerminalAgent};
use sheprd_client::protocol::AgentStatus;
use sheprd_core_client::wire::{StatusState, Terminal as Record};
use sheprd_session::SavedTabs;
use std::{collections::HashSet, io::Read, path::Path};

pub(super) fn read(path: &Path) -> Option<SavedTabs> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(sheprd_session::MAX_SAVED_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    SavedTabs::parse(&bytes).ok()
}

/// [`project_kept`] with `scope`'s saved tabs, as while the shared
/// document stands aside.
#[cfg(test)]
pub(super) fn project(
    boot: &str,
    records: &[Record],
    saved: Option<&SavedTabs>,
    scope: Option<&str>,
) -> Layout {
    project_kept(boot, records, Kept::file(saved, scope))
}

/// The core's terminals in the workspaces and tabs `kept` names, and the
/// rest where their labels say or with the terminals nothing holds.
pub(super) fn project_kept(boot: &str, records: &[Record], kept: Option<Kept<'_>>) -> Layout {
    let mut layout = Layout {
        boot: format!("core:{boot}"),
        connected: true,
        working: working(records),
        workspaces: Vec::new(),
    };
    // The app's own Herdr 2 workspaces come first, in the user's order,
    // with their names and folders, even before they hold a tab.
    for workspace in kept.into_iter().flat_map(Kept::workspaces) {
        layout.workspaces.push(Workspace {
            branch: None,
            chats: Vec::new(),
            cwd: workspace.cwd.to_owned(),
            id: workspace.id.to_owned(),
            label: workspace.label.to_owned(),
            session: None,
            terminals: Vec::new(),
            worktree: None,
        });
    }
    let mut assigned = HashSet::new();
    for tab in kept.into_iter().flat_map(Kept::tabs) {
        let members: Vec<&Record> = tab
            .panes
            .terminals()
            .into_iter()
            .filter_map(|id| records.iter().find(|record| record.address == id.as_str()))
            .filter(|record| assigned.insert(record.address.as_str()))
            .collect();
        if let Some(mut terminal) = terminal(
            boot,
            &tab.target,
            tab.name.unwrap_or(tab.title),
            &members,
            urgency,
        ) {
            terminal.zoomed = tab.zoomed;
            if let Some(active) = tab.active {
                focus(&mut terminal, boot, active);
            }
            // A pane the user named shows that name, a chat's included.
            if let Some(names) = tab.names {
                for (pane, record) in terminal.panes.iter_mut().zip(&members) {
                    if let Some(name) = names.get(&record.address) {
                        pane.label = Some(name.clone());
                    }
                }
            }
            add(&mut layout, tab.workspace_id, terminal);
        }
    }
    for record in records
        .iter()
        .filter(|record| !assigned.contains(record.address.as_str()))
    {
        let tab = pane_id(boot, &record.address);
        // Started for a workspace, as by a phone: listed there before Sheprd
        // puts it in a tab of its own.
        if let Some(cwd) = sheprd_session::new_workspace(&record.labels) {
            // A phone's new workspace, until Sheprd keeps one for it.
            if let Some(terminal) = terminal(boot, &tab, "", &[record], urgency) {
                add_new_workspace(&mut layout, cwd, terminal);
            }
            continue;
        }
        let workspace = sheprd_session::labelled_workspace(&record.labels).map_or_else(
            || unlabelled_workspace(&layout, record).to_owned(),
            |labelled| crate::hub::sessions::join_id(labelled.session, labelled.id),
        );
        if let Some(terminal) = terminal(boot, &tab, "", &[record], urgency) {
            add(&mut layout, &workspace, terminal);
        }
    }
    distinguish_unlisted(&mut layout);
    layout
}

/// Where a terminal with no workspace label is listed: the shell the app
/// shows in place of Herdr 1 in the workspace it shows it in, as the
/// desktop does, and any other with the terminals no tab holds.
fn unlabelled_workspace(layout: &Layout, record: &Record) -> &'static str {
    let stand_in = record.name.as_deref() == Some(sheprd_session::STAND_IN_SHELL)
        && layout
            .workspace(sheprd_session::DEFAULT_WORKSPACE)
            .is_some();
    if stand_in {
        sheprd_session::DEFAULT_WORKSPACE
    } else {
        super::UNLISTED
    }
}

/// What the terminals no tab holds are called beside a workspace already
/// named as they would be, such as the "Herdr 2" a host with no Herdr 1
/// keeps its tabs in.
const UNLISTED_BESIDE_NAMESAKE: &str = "Other terminals";

fn distinguish_unlisted(layout: &mut Layout) {
    let Some(index) = layout
        .workspaces
        .iter()
        .position(|row| row.id == super::UNLISTED)
    else {
        return;
    };
    let label = &layout.workspaces[index].label;
    let namesake = layout
        .workspaces
        .iter()
        .any(|row| row.id != super::UNLISTED && row.label == *label);
    if namesake {
        UNLISTED_BESIDE_NAMESAKE.clone_into(&mut layout.workspaces[index].label);
    }
}

/// Puts the workspaces the app keeps first, in the user's order, ahead of
/// a document's spaces too; the others keep their order.
pub(super) fn lead_kept(layout: &mut Layout, kept: Option<Kept<'_>>) {
    let kept: Vec<String> = kept
        .into_iter()
        .flat_map(Kept::workspaces)
        .map(|workspace| workspace.id.to_owned())
        .collect();
    layout.workspaces.sort_by_key(|workspace| {
        kept.iter()
            .position(|id| *id == workspace.id)
            .unwrap_or(kept.len())
    });
}

/// A phone's new workspace in `cwd`, named for its folder, listed apart.
/// More work there, labelled with the same folder, joins it.
fn add_new_workspace(layout: &mut Layout, cwd: &str, terminal: Terminal) {
    let id = super::new_workspace_id(cwd);
    if let Some(row) = layout.workspaces.iter_mut().find(|row| row.id == id) {
        row.terminals.push(terminal);
        return;
    }
    let label = Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(cwd)
        .to_owned();
    layout.workspaces.push(Workspace {
        branch: None,
        chats: Vec::new(),
        cwd: cwd.to_owned(),
        id,
        label,
        session: None,
        terminals: vec![terminal],
        worktree: None,
    });
}

fn add(layout: &mut Layout, workspace: &str, terminal: Terminal) {
    if let Some(row) = layout.workspaces.iter_mut().find(|row| row.id == workspace) {
        row.terminals.push(terminal);
        return;
    }
    layout.workspaces.push(Workspace {
        branch: None,
        chats: Vec::new(),
        cwd: terminal.cwd.clone().unwrap_or_default(),
        id: workspace.to_owned(),
        label: label(workspace, terminal.cwd.as_deref()),
        session: None,
        terminals: vec![terminal],
        worktree: None,
    });
}

/// What a phone calls a core workspace. A saved tab's workspace id is an
/// opaque Herdr 1 id, shown only if Herdr 1 no longer lists it: its folder
/// reads better.
fn label(workspace: &str, cwd: Option<&str>) -> String {
    (workspace != super::UNLISTED)
        .then_some(cwd)
        .flatten()
        .and_then(|cwd| Path::new(cwd).file_name()?.to_str())
        .unwrap_or("Herdr 2")
        .to_owned()
}

pub(super) fn status(record: &Record) -> AgentStatus {
    match record.status.as_ref().map(|status| status.state) {
        Some(StatusState::Blocked | StatusState::Error) => AgentStatus::Blocked,
        Some(StatusState::Done) => AgentStatus::Done,
        Some(StatusState::Idle) => AgentStatus::Idle,
        Some(StatusState::Working) => AgentStatus::Working,
        _ => AgentStatus::Unknown,
    }
}

pub(super) fn working(records: &[Record]) -> usize {
    records
        .iter()
        .filter(|record| status(record) == AgentStatus::Working)
        .count()
}

pub(super) fn terminal(
    boot: &str,
    tab: &str,
    title: &str,
    records: &[&Record],
    urgency: impl Fn(AgentStatus) -> u8,
) -> Option<Terminal> {
    let first = *records.first()?;
    let panes = records
        .iter()
        .enumerate()
        .map(|(index, record)| TerminalPane {
            cwd: cwd(record),
            focused: index == 0,
            job: super::job::of(record),
            label: record.name.clone().or_else(|| record.title.clone()),
            pane: pane_id(boot, &record.address),
        })
        .collect();
    let agents: Vec<TerminalAgent> = records
        .iter()
        .filter(|record| record.status.is_some())
        .map(|record| TerminalAgent {
            name: record
                .status
                .as_ref()
                .map(|status| status.app.clone())
                .filter(|name| !name.is_empty()),
            pane: pane_id(boot, &record.address),
            status: status(record),
            title: record
                .status
                .as_ref()
                .map(|status| status.title.clone())
                .filter(|title| !title.is_empty())
                .or_else(|| record.title.clone()),
            changed_ms: None,
        })
        .collect();
    let agent = agents
        .iter()
        .max_by_key(|agent| urgency(agent.status))
        .cloned();
    Some(Terminal {
        agent,
        agents,
        // Only this computer's terminals: an agent another machine runs
        // keeps its session records there, out of the daemon's reach.
        convert: records
            .iter()
            .filter(|record| !record.address.contains('/'))
            .find_map(|record| {
                let info = sheprd_core_client::terminal_info(record).ok()?;
                let agent = sheprd_core_client::convert::movable(&info).ok()?;
                Some((pane_id(boot, &record.address), agent))
            }),
        cwd: cwd(first),
        label: if title.is_empty() {
            first
                .name
                .as_ref()
                .or(first.title.as_ref())
                .unwrap_or(&first.address)
                .clone()
        } else {
            title.to_owned()
        },
        panes,
        tab: tab.to_owned(),
        zoomed: false,
    })
}

pub(super) fn cwd(record: &Record) -> Option<String> {
    record
        .foreground
        .as_ref()
        .map(|foreground| &foreground.cwd)
        .filter(|cwd| !cwd.is_empty())
        .or(Some(&record.cwd))
        .filter(|cwd| !cwd.is_empty())
        .cloned()
}

/// Keep tree order while identifying the selected pane. A missing terminal
/// leaves the first live pane selected until the next layout cleanup.
pub(super) fn focus(terminal: &mut Terminal, boot: &str, active: &sheprd_backend::TerminalId) {
    let active = pane_id(boot, active.as_str());
    if terminal.panes.iter().any(|pane| pane.pane == active) {
        for pane in &mut terminal.panes {
            pane.focused = pane.pane == active;
        }
    }
}

pub(super) fn urgency(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Working => 3,
        AgentStatus::Done => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}
