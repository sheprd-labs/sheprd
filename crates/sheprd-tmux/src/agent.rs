//! A chat publishes its own status to the owning tmux session, so a closed
//! or detached view does not interrupt status updates.
use super::{Error, Result, Tmux, URL_SAFE_NO_PAD};
use base64::Engine as _;
use sheprd_backend::{AgentStatus, TerminalId};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AgentReport {
    pub pid: u32,
    pub status: AgentStatus,
    pub display: String,
    pub title: Option<String>,
    pub endpoint: Option<String>,
}

impl Tmux {
    /// The app's terminal this process was started in.
    pub fn caller() -> Option<TerminalId> {
        Self::caller_in(|key| std::env::var(key).ok())
    }

    pub fn caller_in(var: impl Fn(&str) -> Option<String>) -> Option<TerminalId> {
        if ["HC_TERMINAL_ID", "HERDR_PANE_ID"]
            .iter()
            .any(|key| var(key).is_some_and(|id| !id.is_empty()))
        {
            return None;
        }
        TerminalId::new(var("SHEPRD_TERMINAL_ID")?)
            .ok()
            .filter(TerminalId::is_tmux)
    }

    pub fn report(&self, id: &TerminalId, report: Option<&AgentReport>) -> Result<()> {
        let mut cmd = self.command();
        cmd.args(["set-environment", "-t", &Self::target(id)?]);
        if let Some(report) = report {
            let bytes = serde_json::to_vec(report).map_err(|_| Error::Invalid)?;
            if bytes.len() > 8192 {
                return Err(Error::Invalid);
            }
            cmd.args(["SHEPRD_TMUX_REPORT", &URL_SAFE_NO_PAD.encode(bytes)]);
        } else {
            cmd.args(["-u", "SHEPRD_TMUX_REPORT"]);
        }
        self.run(cmd).map(drop)
    }

    /// The PID of the process tmux started in this pane, the root from which
    /// the system process monitor finds all descendants.
    pub fn root_pid(&self, id: &TerminalId) -> Result<u32> {
        let mut cmd = self.command();
        cmd.args([
            "display-message",
            "-p",
            "-t",
            &format!("{}:", Self::target(id)?),
            "#{pane_pid}",
        ]);
        let out = self.run(cmd)?;
        std::str::from_utf8(&out)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .filter(|pid| *pid > 0)
            .ok_or(Error::Invalid)
    }
}
