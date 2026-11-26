#![doc = include_str!("../README.md")]
mod agent;
pub use agent::AgentReport;
mod control;
mod host;
pub use host::{CommandHost, CommandTask};
mod recovery;
pub use recovery::ChatRecovery;
mod terminal;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sheprd_backend::{AgentStatus, Backend, Role, RunState, Size, Spawn, TerminalId, TerminalInfo};
use std::{collections::BTreeMap, path::PathBuf, process::Command, time::Duration};
use uuid::Uuid;

pub use terminal::TmuxTerminal;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tmux: {0}")]
    Command(#[from] sheprd_client::Error),
    #[error("tmux control client: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid tmux terminal identity or server response")]
    Invalid,
    #[error(transparent)]
    Id(#[from] sheprd_backend::Error),
    #[error(transparent)]
    Terminal(#[from] sheprd_terminal::Error),
    #[error("tmux terminal already exists")]
    NameTaken,
    #[error("the tmux pane is missing")]
    MissingPane,
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct Tmux {
    program: PathBuf,
    target: Option<std::sync::Arc<dyn CommandHost>>,
    socket: String,
    scrollback: usize,
}

const PREFIX: &str = "tmux-";

impl Tmux {
    pub fn new(scrollback: usize) -> Self {
        Self {
            program: "tmux".into(),
            target: None,
            socket: "sheprd".into(),
            scrollback,
        }
    }

    fn command(&self) -> Command {
        let mut cmd = sheprd_client::new_command(&self.program);
        if self.target.is_none() {
            sheprd_client::login_env::apply(&mut cmd);
        }
        cmd.env_remove("TMUX")
            .args(["-u", "-L", &self.socket, "-f", "/dev/null"]);
        cmd
    }

    fn target(id: &TerminalId) -> Result<String> {
        id.is_tmux().then(|| format!("={id}")).ok_or(Error::Invalid)
    }

    fn run(&self, command: Command) -> Result<Vec<u8>> {
        self.with_command(command, false, Self::output)
    }

    fn output(command: Command) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        sheprd_client::run_script_command(
            command,
            std::io::empty(),
            &mut out,
            sheprd_client::ScriptLimits {
                output: 4 << 20,
                idle: Duration::from_secs(5),
            },
            &false.into(),
        )?;
        Ok(out)
    }

    pub fn named_id(name: &str) -> Result<TerminalId> {
        Ok(TerminalId::new(format!(
            "{PREFIX}{}",
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("sheprd:tmux:{name}").as_bytes()
            )
        ))?)
    }
}

impl Tmux {
    fn spawn_id(&self, id: TerminalId, spawn: &Spawn, guard: Option<&str>) -> Result<TerminalInfo> {
        let metadata = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&(&spawn.name, &spawn.labels)).map_err(|_| Error::Invalid)?);
        let mut cmd = self.command();
        cmd.args(["start-server", ";"]);
        if self.scrollback > 0 {
            cmd.args([
                "set-option",
                "-g",
                "history-limit",
                &self.scrollback.clamp(1, 100_000).to_string(),
                ";",
            ]);
        }
        cmd.args(["set-option", "-g", "status", "off", ";"]);
        cmd.args([
            "new-session",
            "-d",
            "-s",
            id.as_str(),
            "-x",
            "80",
            "-y",
            "24",
        ]);
        // tmux parses trailing semicolons even in argv. Quote the whole
        // program once for the shell and keep user data out of tmux syntax.
        let mut shell = guard.unwrap_or_default().to_owned();
        if let Some(cwd) = &spawn.cwd {
            shell.push_str(&format!(
                "cd {} || exit 1\n",
                sheprd_client::shell_quote(cwd.to_str().ok_or(Error::Invalid)?)
            ));
        }
        if spawn.argv.is_empty() {
            shell.push_str(&self.keep_shell_script(&id, spawn)?);
        }
        shell.push_str(
            "unset HC_TERMINAL_ID HC_TERMINAL_SOCKET HERDR_PANE_ID HERDR_TAB_ID HERDR_WORKSPACE_ID HERDR_SOCKET_PATH
exec /usr/bin/env --",
        );
        for (key, value) in &spawn.env {
            if key.is_empty() || key.contains('=') || key.starts_with('-') {
                return Err(Error::Invalid);
            }
            shell.push(' ');
            shell.push_str(&sheprd_client::shell_quote(&format!("{key}={value}")));
        }
        shell.push_str(&format!(" SHEPRD_TERMINAL_ID={id}"));
        if spawn.argv.is_empty() {
            shell.push_str(" \"${SHELL:-/bin/sh}\" -l");
        } else {
            for word in &spawn.argv {
                shell.push(' ');
                shell.push_str(&sheprd_client::shell_quote(word));
            }
        }
        // Force POSIX shell syntax even when the login shell is fish.
        cmd.args(["-e", &format!("SHEPRD_TMUX_METADATA={metadata}")]);
        if !spawn.argv.is_empty() && guard.is_none() {
            // An archive script may finish before the GUI attaches. Create an
            // empty pane and enable retention before starting its program.
            let pane = format!("={}:", id.as_str());
            cmd.args([
                "",
                ";",
                "set-option",
                "-w",
                "-t",
                &pane,
                "remain-on-exit",
                "on",
                ";",
                "respawn-pane",
                "-k",
                "-t",
                &pane,
            ]);
        }
        cmd.args(["--", "/bin/sh", "-c", &shell]);
        match self.run(cmd) {
            Err(Error::Command(sheprd_client::Error::ScriptExit { stderr, .. }))
                if stderr.contains("duplicate session:") =>
            {
                return Err(Error::NameTaken);
            }
            result => {
                result?;
            }
        }
        Ok(TerminalInfo {
            id,
            agent: AgentStatus::Unknown,
            cwd: spawn.cwd.clone(),
            foreground: None,
            labels: spawn.labels.clone(),
            name: spawn.name.clone(),
            reporter: None,
            state: RunState::Running,
            title: None,
        })
    }
}

fn missing(error: &Error) -> bool {
    no_server(error)
        || matches!(error, Error::MissingPane)
        || matches!(error, Error::Command(sheprd_client::Error::ScriptExit {stderr, ..})
        if stderr.contains("can't find session:") || stderr.contains("can't find pane:"))
}

fn no_server(error: &Error) -> bool {
    matches!(error, Error::Command(sheprd_client::Error::ScriptExit {stderr, ..})
        if stderr.contains("no server running") || stderr.contains("No such file or directory") && stderr.contains("connecting to"))
}

type Metadata = (Option<String>, BTreeMap<String, String>);

// tmux renders tabs as underscores for clients in the C locale. Use a printable
// delimiter and keep the path last so it may itself contain that delimiter.
const FORMAT: &str = "#{session_name}|#{pane_dead}|#{pane_dead_status}|#{SHEPRD_TMUX_METADATA}|#{SHEPRD_TMUX_REPORT}|#{?#{m/r:[[:cntrl:]|],#{pane_current_command}},,#{pane_current_command}}|#{?#{m/r:[[:cntrl:]],#{pane_current_path}},,#{pane_current_path}}";

fn record(row: &str) -> Option<TerminalInfo> {
    let mut fields = row.splitn(7, '|');
    let id = TerminalId::new(fields.next()?)
        .ok()
        .filter(TerminalId::is_tmux)?;
    let dead = fields.next()? == "1";
    let code = fields.next()?.parse().ok();
    let state = if dead {
        RunState::Exited(sheprd_backend::Exit { code })
    } else {
        RunState::Running
    };
    let bytes = URL_SAFE_NO_PAD.decode(fields.next()?).ok()?;
    let (name, mut labels): Metadata = serde_json::from_slice(&bytes).ok()?;
    let report = URL_SAFE_NO_PAD
        .decode(fields.next()?)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AgentReport>(&bytes).ok());
    let program = fields.next()?;
    if let Some(endpoint) = report.as_ref().and_then(|report| report.endpoint.as_ref()) {
        labels.insert(sheprd_chat_protocol::TOKEN.to_owned(), endpoint.clone());
    }
    let agent = report
        .as_ref()
        .map(|r| r.status)
        .unwrap_or(AgentStatus::Unknown);
    let title = report.as_ref().and_then(|r| r.title.clone());
    let foreground = report
        .as_ref()
        .filter(|r| r.pid > 0 && matches!(program, "sheprd-acp" | "herdr-acp"))
        .map(|r| sheprd_backend::Process {
            pid: r.pid,
            name: program.to_owned(),
            argv: Vec::new(),
        });
    let reporter = report.map(|r| r.display).or_else(|| {
        sheprd_client::herdr::AgentKind::identify(None, program, &[])
            .map(|kind| kind.name().to_owned())
    });
    let cwd = fields.next()?;
    Some(TerminalInfo {
        agent,
        // tmux reports a POSIX path even when the desktop runs on Windows.
        cwd: cwd.starts_with('/').then(|| PathBuf::from(cwd)),
        foreground,
        id,
        labels,
        name,
        reporter,
        state,
        title,
    })
}

impl Backend for Tmux {
    type Error = Error;
    type Terminal = TmuxTerminal;

    fn terminals(&self) -> Result<Vec<TerminalInfo>> {
        let mut command = self.command();
        command.args(["list-sessions", "-F", FORMAT]);
        match self.run(command) {
            Ok(bytes) => Ok(String::from_utf8_lossy(&bytes)
                .lines()
                .filter_map(record)
                .collect()),
            Err(error) if no_server(&error) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn spawn(&self, spawn: &Spawn) -> Result<TerminalInfo> {
        let id = match &spawn.name {
            Some(name) => Self::named_id(name)?,
            None => TerminalId::new(format!("{PREFIX}{}", Uuid::new_v4()))?,
        };
        if spawn.name.is_some()
            && spawn.argv.is_empty()
            && let Some(saved) = self.recover(&id)?
        {
            return Ok(saved);
        }
        self.spawn_id(id, spawn, None)
    }

    fn attach(&self, id: &TerminalId, size: Size, role: Role) -> Result<TmuxTerminal> {
        match TmuxTerminal::attach(self, id, size, role) {
            Err(error) if role == Role::Active && missing(&error) => match self.recover(id) {
                Ok(Some(_)) | Err(Error::NameTaken) => TmuxTerminal::attach(self, id, size, role),
                Ok(None) => Err(error),
                Err(other) => Err(other),
            },
            result => result,
        }
    }

    fn kill(&self, id: &TerminalId) -> Result<()> {
        let mut cmd = self.command();
        cmd.args(["kill-session", "-t", &Self::target(id)?]);
        // Fence a recovery that read its recipe but has not started its pane.
        self.forget(id)?;
        match self.run(cmd) {
            Err(error) if no_server(&error) => Ok(()),
            Err(Error::Command(sheprd_client::Error::ScriptExit { stderr, .. }))
                if stderr.contains("can't find session:") =>
            {
                Ok(())
            }
            other => other.map(drop),
        }?;
        self.forget(id)
    }
}

#[cfg(test)]
mod tests;
