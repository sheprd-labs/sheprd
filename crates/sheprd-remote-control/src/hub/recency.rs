//! When each agent last changed state, on the wall clock, so a phone can list
//! agents in terminal tabs by recency across computers. Herdr's
//! `state_change_seq` only orders one daemon's own changes and carries no
//! time, so the time is what this daemon saw it move. An agent already there
//! when the daemon first sees a boot has no known time.

use super::layout::Layout;
use sheprd_client::protocol::ClientShellSnapshot;
use std::collections::HashMap;

/// Each listed agent's sequence and the time it last moved, for one boot.
#[derive(Debug, Default)]
pub(super) struct AgentTimes {
    boot: String,
    panes: HashMap<String, (u64, Option<u64>)>,
}

impl AgentTimes {
    /// Follows `agents`, as pane and sequence, seen at `now_ms`. A new boot
    /// restarts every sequence, so it starts over rather than reading that
    /// as every agent changing at once. Only agents still listed are kept.
    fn observe<'a>(
        &mut self,
        boot: &str,
        agents: impl IntoIterator<Item = (&'a str, u64)>,
        now_ms: u64,
    ) {
        let known = self.boot == boot;
        if !known {
            boot.clone_into(&mut self.boot);
            self.panes.clear();
        }
        let panes = agents
            .into_iter()
            .map(|(pane, seq)| {
                let changed = match self.panes.get(pane) {
                    Some((old, changed)) if *old == seq => *changed,
                    Some(_) => Some(now_ms),
                    // One that appears later in a boot just started.
                    None => known.then_some(now_ms),
                };
                (pane.to_owned(), (seq, changed))
            })
            .collect();
        self.panes = panes;
    }

    fn changed_ms(&self, pane: &str) -> Option<u64> {
        self.panes.get(pane).and_then(|(_, changed)| *changed)
    }

    /// Follows `snapshot` and dates the terminal agents of its `layout`.
    pub(super) fn stamp(
        &mut self,
        snapshot: &ClientShellSnapshot,
        layout: &mut Layout,
        now_ms: u64,
    ) {
        let agents = snapshot
            .agents
            .iter()
            .map(|agent| (agent.pane_id.as_str(), agent.state_change_seq));
        self.observe(&snapshot.boot_id, agents, now_ms);
        let terminals = layout
            .workspaces
            .iter_mut()
            .flat_map(|workspace| workspace.terminals.iter_mut());
        for terminal in terminals {
            for agent in terminal.agent.iter_mut().chain(&mut terminal.agents) {
                agent.changed_ms = self.changed_ms(&agent.pane);
            }
        }
    }

    /// Dates the terminal agents of herdr-core's `layout`, from core `boot`.
    /// A core reports no sequence, so a change of status is the move.
    pub(super) fn stamp_statuses(&mut self, boot: &str, layout: &mut Layout, now_ms: u64) {
        let terminals = || {
            layout
                .workspaces
                .iter()
                .flat_map(|workspace| workspace.terminals.iter())
                .flat_map(|terminal| terminal.agent.iter().chain(&terminal.agents))
        };
        let agents: Vec<(String, u64)> = terminals()
            .map(|agent| (agent.pane.clone(), status_seq(agent.status)))
            .collect();
        self.observe(
            boot,
            agents.iter().map(|(pane, seq)| (pane.as_str(), *seq)),
            now_ms,
        );
        let terminals = layout
            .workspaces
            .iter_mut()
            .flat_map(|workspace| workspace.terminals.iter_mut());
        for terminal in terminals {
            for agent in terminal.agent.iter_mut().chain(&mut terminal.agents) {
                agent.changed_ms = self.changed_ms(&agent.pane);
            }
        }
    }
}

/// A status as the sequence [`AgentTimes`] follows.
fn status_seq(status: sheprd_client::protocol::AgentStatus) -> u64 {
    use sheprd_client::protocol::AgentStatus;
    match status {
        AgentStatus::Idle => 0,
        AgentStatus::Working => 1,
        AgentStatus::Blocked => 2,
        AgentStatus::Done => 3,
        AgentStatus::Unknown => 4,
    }
}

#[cfg(test)]
mod tests;
