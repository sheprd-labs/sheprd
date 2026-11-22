//! Whether a window's agents are still working, as its daemons report them.

use sheprd_client::protocol::{AgentStatus, ClientShellSnapshot};

/// What one endpoint's agents are doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Work {
    Working,
    /// Every agent is idle, blocked on the user, or done.
    Idle,
    /// The endpoint has no snapshot yet or is reconnecting, so its agents
    /// cannot be read.
    Unknown,
}

impl Work {
    /// Takes an endpoint's snapshot, `None` while it is not connected. A
    /// blocked agent waits on the user, so it keeps nothing awake.
    pub(crate) fn of(snapshot: Option<&ClientShellSnapshot>) -> Self {
        match snapshot {
            Some(snapshot)
                if snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.agent_status == AgentStatus::Working) =>
            {
                Self::Working
            }
            Some(_) => Self::Idle,
            None => Self::Unknown,
        }
    }
}
