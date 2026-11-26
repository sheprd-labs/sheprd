//! Program resume (OSC 7577): tells the terminal how to start this chat
//! again, so a terminal that restores its panes after a restart, as Herdr
//! does, can bring the chat back itself, with no client watching.
//!
//! The terminal gives each pane's first process a secret in
//! [`TOKEN_ENV`]; its presence is the only sign the terminal reads these
//! reports. A report names this process, the time, the command that
//! continues the agent's current session in its thread, and the folder, and
//! is signed with the token. The chat reports again whenever its session
//! changes, and clears the report only when it ends on its own, never when
//! it is killed or loses its terminal: that is when a resume is wanted.
//!
//! The token is this program's alone; [`crate::process`] keeps it from the
//! agent, whose own resume report would replace the chat with the agent's
//! terminal interface.

use std::{
    ffi::OsStr,
    io::Write,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest as _, Sha256};
use sheprd_chat_protocol::{Agent, restore, thread::ThreadId};

use crate::cli::{AgentChoice, Options};

/// The variable the terminal passes its token in.
pub(crate) const TOKEN_ENV: &str = "OSC7577_TOKEN";
/// The longest report every terminal must accept, introducer and terminator
/// included.
const MAX_REPORT: usize = 2048;
const MAX_ARGV: usize = 64;

/// How this chat is started again, fixed for its life except for the agent,
/// which a switch changes, and the session, which each report names.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Launch {
    adapter: bool,
    agent: Agent,
    attach: bool,
    cwd: String,
    headless: bool,
    program: String,
    thread: Option<ThreadId>,
}

impl Launch {
    /// How the chat `options` asked for is started again by `program`;
    /// `adapter` is the choice the chat made, its environment's included.
    fn new(
        program: &OsStr,
        options: &Options,
        adapter: bool,
        cwd: &Path,
        thread: Option<ThreadId>,
    ) -> Option<Self> {
        let AgentChoice::Builtin { agent, args } = &options.agent else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let program = text(program).filter(|_| Path::new(program).is_absolute())?;
        let cwd = text(cwd.as_os_str()).filter(|_| cwd.is_absolute())?;
        Some(Self {
            adapter,
            agent: *agent,
            attach: options.attach,
            cwd,
            headless: options.headless,
            program,
            thread,
        })
    }

    /// The command that continues `session`, as `sheprd-acp`'s own restore
    /// types it, or `None` when it could not be typed back safely.
    fn argv(&self, session: &str) -> Option<Vec<String>> {
        if !restore::session_id_is_plain(session) {
            return None;
        }
        let mut argv = vec![
            self.program.clone(),
            "--resume".to_owned(),
            session.to_owned(),
        ];
        if let Some(thread) = self.thread {
            argv.extend(["--thread".to_owned(), thread.to_string()]);
        }
        if self.adapter {
            argv.push("--acp".to_owned());
        }
        if self.headless {
            argv.push("--headless".to_owned());
        }
        if !self.attach {
            argv.push("--no-attach".to_owned());
        }
        argv.push(self.agent.name().to_owned());
        Some(argv)
    }
}

/// The reports one chat sends, written to `W`, the pane.
pub(crate) struct Resumer<W> {
    launch: Launch,
    out: W,
    pid: u32,
    /// Whether the terminal may hold a record of this chat to clear.
    reported: bool,
    /// The time of the last report: each must be later than the one before.
    time: u64,
    token: String,
}

/// A resumer the session and the end of the chat share.
pub(crate) type Shared = Arc<Mutex<Resumer<std::io::Stdout>>>;

/// The resumer of a chat in a terminal that reads reports, or `None`: a
/// custom command, extra agent arguments, or a program path or folder a
/// report cannot carry keep nothing. `adapter` is the chat's effective
/// choice, `HERDR_ACP_ADAPTER` included, which a restarted pane may not set.
pub(crate) fn from_env(
    options: &Options,
    adapter: bool,
    cwd: &Path,
    thread: Option<ThreadId>,
) -> Option<Shared> {
    use std::io::IsTerminal as _;
    let out = std::io::stdout();
    if !out.is_terminal() {
        return None;
    }
    let token = std::env::var_os(TOKEN_ENV)?;
    let program = std::env::current_exe().ok()?;
    let launch = Launch::new(program.as_os_str(), options, adapter, cwd, thread)?;
    let resumer = Resumer::new(&token, launch, std::process::id(), out)?;
    Some(Arc::new(Mutex::new(resumer)))
}

/// Runs `f` on the shared resumer, if there is one.
pub(crate) fn with(shared: Option<&Shared>, f: impl FnOnce(&mut Resumer<std::io::Stdout>)) {
    if let Some(shared) = shared {
        f(&mut shared.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

impl<W: Write> Resumer<W> {
    fn new(token: &OsStr, launch: Launch, pid: u32, out: W) -> Option<Self> {
        let token = token.to_str().filter(|token| token_is_valid(token))?;
        Some(Self {
            launch,
            out,
            pid,
            reported: false,
            time: 0,
            token: token.to_owned(),
        })
    }

    /// The agent's session `session` opened: the command now continues it.
    pub(crate) fn opened(&mut self, session: &str) {
        let report = self.launch.argv(session).and_then(|argv| {
            let mut pairs = self.head();
            pairs.push(("argv", encode_argv(&argv)?));
            pairs.push(("cwd", STANDARD.encode(&self.launch.cwd)));
            signed(&pairs, &self.token)
        });
        match report {
            Some(report) => {
                self.reported = true;
                self.send(&report);
            }
            // The terminal's command would continue an earlier session.
            None => self.clear(),
        }
    }

    /// The chat switched to `agent`, whose session opens next. The record
    /// of the agent before goes at once, as the restore record does.
    pub(crate) fn switched(&mut self, agent: Agent) {
        self.launch.agent = agent;
        self.clear();
    }

    /// The chat ended on its own: nothing is left to resume.
    pub(crate) fn ended(&mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        if !self.reported {
            return;
        }
        let mut pairs = self.head();
        pairs.push(("clear", "1".to_owned()));
        if let Some(report) = signed(&pairs, &self.token) {
            self.reported = false;
            self.send(&report);
        }
    }

    /// `pid` and `time`, the pairs every report starts with.
    fn head(&mut self) -> Vec<(&'static str, String)> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        self.time = now.max(self.time.saturating_add(1));
        vec![
            ("pid", self.pid.to_string()),
            ("time", self.time.to_string()),
        ]
    }

    /// Writes `report` in one call, so it lands between, never inside, the
    /// escape sequences the view draws with.
    fn send(&mut self, report: &str) {
        let _ = self.out.write_all(report.as_bytes());
        let _ = self.out.flush();
    }
}

/// A token as the protocol defines one: 22 to 64 URL-safe characters.
fn token_is_valid(token: &str) -> bool {
    (22..=64).contains(&token.len())
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// `value` as text a report may carry: UTF-8 with no control characters.
fn text(value: &OsStr) -> Option<String> {
    let value = value.to_str()?;
    (!value.chars().any(char::is_control)).then(|| value.to_owned())
}

fn encode_argv(argv: &[String]) -> Option<String> {
    if argv.len() > MAX_ARGV || argv.iter().any(|arg| arg.chars().any(char::is_control)) {
        return None;
    }
    let encoded: Vec<String> = argv.iter().map(|arg| STANDARD.encode(arg)).collect();
    Some(encoded.join(","))
}

/// The whole escape sequence for `pairs`, signed with `token`, or `None`
/// when it is longer than every terminal must accept.
fn signed(pairs: &[(&str, String)], token: &str) -> Option<String> {
    let body = pairs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(":");
    let mac = hex(&Sha256::digest(format!("{body}:{token}")));
    let report = format!("\x1b]7577;{body}:mac={mac}\x1b\\");
    (report.len() <= MAX_REPORT).then_some(report)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
