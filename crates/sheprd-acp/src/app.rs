//! Starts the agent and wires the session, the attach socket, Herdr reports,
//! and the in-pane view together.

use std::{path::PathBuf, thread};

use anyhow::Context as _;
use crossbeam_channel::{Sender, bounded};
use sheprd_chat_protocol::{EndpointId, NoticeLevel};

use crate::{
    agents::Launch,
    cli::{AgentChoice, Options},
    harness::{Acp, Pipes, Reach},
    herdr::{Reporter, Target},
    process,
    session::{Config, Flow, HarnessId, Input, Session, Update},
};

/// Inputs waiting for the session thread.
const INPUTS: usize = 1024;

/// Asks for the ACP adapter, as `--acp` does.
const ADAPTER_VARIABLE: &str = "HERDR_ACP_ADAPTER";

/// What a chat runs and shows of its agent.
struct Agent {
    display: String,
    /// How the agent gets the server that reads its thread.
    given: crate::thread_mcp::Given,
    /// What the agent's own log says of a session it will not replay.
    history: Option<crate::history::Result<sheprd_chat_protocol::Transcript>>,
    login_hint: Option<String>,
    start: Start,
}

/// How to start an agent's process and reach it.
struct Start {
    args: Vec<std::ffi::OsString>,
    /// Added to what the agent inherits.
    env: Vec<(&'static str, &'static str)>,
    program: std::ffi::OsString,
    reach: Box<dyn Reach>,
}

/// One started agent: its process, and the outbox the session writes to it.
struct Started {
    outbox: crate::rpc::Outbox,
    process: process::AgentProcess,
}

fn agent(
    options: &Options,
    cwd: &std::path::Path,
    server: Option<&crate::thread_mcp::Server>,
) -> Agent {
    match &options.agent {
        AgentChoice::Builtin { agent, args: extra } => {
            let mut built = builtin(
                *agent,
                options.load.as_deref(),
                adapter(options),
                cwd,
                server,
            );
            built.start.args.extend(extra.iter().cloned());
            if let Some(name) = &options.name {
                built.display.clone_from(name);
            }
            built
        }
        AgentChoice::Command { program, args } => Agent {
            display: options.name.clone().unwrap_or_else(|| {
                PathBuf::from(&program)
                    .file_name()
                    .map_or_else(|| "Agent".to_owned(), |n| n.to_string_lossy().into_owned())
            }),
            given: crate::thread_mcp::Given::Not,
            history: None,
            login_hint: None,
            start: Start {
                args: args.clone(),
                env: Vec::new(),
                program: program.clone(),
                reach: Box::new(Acp),
            },
        },
    }
}

/// A built-in agent, continuing its session `load` when set, given
/// `server` to read its thread with when its harness takes one.
fn builtin(
    agent: crate::agents::Agent,
    load: Option<&str>,
    adapter: bool,
    cwd: &std::path::Path,
    server: Option<&crate::thread_mcp::Server>,
) -> Agent {
    use crate::thread_mcp::Given;
    let path = search_path();
    let mut env = Vec::new();
    let mut history = None;
    let mut given = Given::Not;
    let (program, args, reach): (_, _, Box<dyn Reach>) =
        match crate::agents::launch(agent, Some(&path), adapter) {
            Launch::Native(program) => {
                let (args, reach, launched) = native(agent, load, server);
                if launched {
                    given = Given::Launched;
                }
                history = native_history(agent, load, cwd);
                if matches!(agent, crate::agents::Agent::Claude) {
                    let set = std::env::var_os(crate::claude::TODO_TOOLS);
                    env = crate::claude::env(set.as_deref());
                }
                (program.into_os_string(), args, reach)
            }
            // An ACP agent replays its own history on `session/load`.
            Launch::Acp(program, args) => {
                if let Some(server) = server {
                    given = Given::InSession(server.clone());
                }
                (program, args, Box::new(Acp))
            }
        };
    Agent {
        display: agent.display_name().to_owned(),
        given,
        history,
        login_hint: Some(agent.login_hint().to_owned()),
        start: Start {
            args,
            env,
            program,
            reach,
        },
    }
}

/// Where agents are looked for: `PATH`, then where they install themselves.
fn search_path() -> std::ffi::OsString {
    crate::agents::search_path(
        std::env::var_os("PATH").as_deref(),
        std::env::home_dir().as_deref(),
    )
}

/// Authenticates Antigravity through the ACP method the person selected, or
/// a credential explicitly present in the chat's environment.
fn antigravity_auth_method() -> String {
    let requested = std::env::var("SHEPRD_ANTIGRAVITY_AUTH_METHOD").ok();
    antigravity_auth_method_for(
        requested.as_deref(),
        std::env::var_os("GEMINI_API_KEY").is_some(),
        std::env::var_os("GOOGLE_CLOUD_PROJECT").is_some()
            && std::env::var_os("GOOGLE_CLOUD_LOCATION").is_some(),
    )
}

fn antigravity_auth_method_for(
    requested: Option<&str>,
    has_api_key: bool,
    has_cloud_project: bool,
) -> String {
    if let Some(method) = requested.filter(|method| !method.is_empty()) {
        return method.to_owned();
    }
    if has_api_key {
        return "gemini-api-key".to_owned();
    }
    if has_cloud_project {
        return "agent-platform".to_owned();
    }
    "oauth-personal".to_owned()
}

/// The ACP authentication method Gemini should try when a session asks for
/// sign-in, from an explicit override or configured credentials.
fn gemini_auth_method() -> String {
    let requested = std::env::var("SHEPRD_GEMINI_AUTH_METHOD").ok();
    let use_vertex = std::env::var("GOOGLE_GENAI_USE_VERTEXAI")
        .ok()
        .is_some_and(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "0" | "false"
            )
        });
    gemini_auth_method_for(
        requested.as_deref(),
        std::env::var_os("GEMINI_API_KEY").is_some(),
        use_vertex
            || (std::env::var_os("GOOGLE_CLOUD_PROJECT").is_some()
                && std::env::var_os("GOOGLE_CLOUD_LOCATION").is_some())
            || std::env::var_os("GOOGLE_API_KEY").is_some(),
        std::env::var_os("GOOGLE_GEMINI_BASE_URL").is_some(),
    )
}

fn gemini_auth_method_for(
    requested: Option<&str>,
    has_api_key: bool,
    has_vertex: bool,
    has_gateway: bool,
) -> String {
    if let Some(method) = requested.filter(|method| !method.is_empty()) {
        return method.to_owned();
    }
    if has_api_key {
        return "gemini-api-key".to_owned();
    }
    if has_vertex {
        return "vertex-ai".to_owned();
    }
    if has_gateway {
        return "gateway".to_owned();
    }
    "oauth-personal".to_owned()
}

/// The agents a `--choose` chat offers: those installed here, and the one
/// it starts with even when only a fetcher such as `npx` can run it.
fn choices(agent: crate::agents::Agent) -> Vec<crate::agents::Agent> {
    let installed = crate::agents::installed(Some(&search_path()));
    choices_from(agent, &installed)
}

fn choices_from(
    agent: crate::agents::Agent,
    installed: &[crate::agents::Agent],
) -> Vec<crate::agents::Agent> {
    crate::agents::Agent::ALL
        .iter()
        .copied()
        .filter(|choice| *choice == agent || installed.contains(choice))
        .collect()
}

/// Starts `start` in `cwd` as `harness`: what the agent says, and its end,
/// reach the session through `inputs`, marked as this harness's. The agent
/// is told the endpoint of `chat`, the chat it runs in, when it listens.
fn start(
    start: Start,
    cwd: &std::path::Path,
    inputs: &Sender<Input>,
    harness: HarnessId,
    chat: Option<&EndpointId>,
) -> crate::Result<Started> {
    let mut env = start.env.clone();
    if let Some(chat) = chat {
        env.push((sheprd_chat_protocol::AGENT_CHAT_ENV, chat.as_str()));
    }
    let spawned = process::spawn(&start.program, &start.args, &env, cwd)?;
    let process = spawned.process.clone();
    let to_session = inputs.clone();
    let wired = start.reach.connect(
        Pipes {
            input: Box::new(spawned.input),
            output: Box::new(spawned.output),
        },
        Box::new(move |inbound| to_session.send(Input::Agent(harness, inbound)).is_ok()),
    );
    let waiter = inputs.clone();
    let watched = process.clone();
    let waiting = thread::Builder::new()
        .name("acp-wait".to_owned())
        .spawn(move || {
            let code = watched.wait().ok().and_then(|status| status.code());
            let stderr = watched.stderr_tail();
            let _ = waiter.send(Input::AgentExited {
                code,
                harness,
                stderr,
            });
        });
    match (wired, waiting) {
        (Ok(outbox), Ok(_)) => Ok(Started { outbox, process }),
        (Err(error), _) | (_, Err(error)) => {
            process.stop();
            Err(error.into())
        }
    }
}

/// Stops an agent the chat switched away from, on a thread of its own so
/// its grace period never holds up the session. The program joins the
/// thread before it exits, so an agent that ignores the first request is
/// still killed rather than outliving the chat.
fn stop_later(process: process::AgentProcess) -> Option<thread::JoinHandle<()>> {
    let stopping = thread::Builder::new().name("acp-stop".to_owned()).spawn({
        let process = process.clone();
        move || process.stop()
    });
    match stopping {
        Ok(handle) => Some(handle),
        Err(_) => {
            process.stop();
            None
        }
    }
}

/// The settings the client that started this chat copied from another,
/// for `harness` when it is the agent they were read from. A continued
/// session keeps its own; a value that cannot be read says so in the chat.
fn carried_settings(
    harness: Option<crate::agents::Agent>,
    continues: bool,
    notices: &mut Vec<(NoticeLevel, String)>,
) -> Option<sheprd_chat_protocol::Settings> {
    let text = std::env::var(sheprd_chat_protocol::SETTINGS_ENV).ok()?;
    let harness = harness.filter(|_| !continues)?;
    match sheprd_chat_protocol::Settings::decode(&text) {
        Ok(settings) => Some(settings).filter(|settings| settings.is_for(harness)),
        Err(error) => {
            notices.push((
                NoticeLevel::Warning,
                format!("This chat starts with its agent's own settings: {error}"),
            ));
            None
        }
    }
}

/// Whether the chat reaches a built-in agent through its ACP adapter.
fn adapter(options: &Options) -> bool {
    options.adapter || std::env::var_os(ADAPTER_VARIABLE).is_some_and(|value| value == "1")
}

/// The arguments that start `agent`'s own program, and the translator that
/// reaches it, continuing session `load` when set; `true` when they give it
/// `server`, which Pi takes no way to.
fn native(
    agent: crate::agents::Agent,
    load: Option<&str>,
    server: Option<&crate::thread_mcp::Server>,
) -> (Vec<std::ffi::OsString>, Box<dyn Reach>, bool) {
    use crate::agents::Agent;
    match agent {
        Agent::Codex => {
            let mut args: Vec<std::ffi::OsString> = server
                .map(|server| server.codex_overrides().map(Into::into).to_vec())
                .unwrap_or_default();
            args.extend(crate::codex::args());
            (args, crate::codex::reach(), server.is_some())
        }
        Agent::Pi => (crate::pi::args(load), crate::pi::reach(), false),
        // `launch` reaches no other agent natively; Claude is the default.
        _ => {
            let session = load.map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned);
            let mut args = crate::claude::args(&session, load.is_some());
            if let Some(server) = server {
                args.extend(["--mcp-config".into(), server.claude_config().into()]);
            }
            (args, crate::claude::reach(session), server.is_some())
        }
    }
}

/// What a natively reached agent's log says of session `load`, for an agent
/// whose log this program can read.
fn native_history(
    agent: crate::agents::Agent,
    load: Option<&str>,
    cwd: &std::path::Path,
) -> Option<crate::history::Result<sheprd_chat_protocol::Transcript>> {
    let agent = crate::mirror::transcript::TranscriptAgent::try_from(agent).ok()?;
    Some(crate::history::load(agent, load?, cwd))
}

pub(crate) fn run(options: Options) -> anyhow::Result<()> {
    let _owner = crate::restore::core::lock()?;
    let claimed = crate::restore::core::claim_launch()?;
    run_owned(claimed, options)
}

/// The caller holds the core endpoint lock before reading recovery state or
/// spawning any agent. Herdr 1 chats retain their existing ownership policy.
/// `claimed` is the recovery generation this launch claimed, if any.
pub(crate) fn run_owned(
    claimed: Option<crate::restore::core::Claimed>,
    options: Options,
) -> anyhow::Result<()> {
    let cwd = match options.cwd.clone() {
        Some(cwd) => cwd,
        None => std::env::current_dir().context("could not read the current directory")?,
    };
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("no directory {}", cwd.display()))?;
    // A built-in agent's chat is a thread, `--thread`'s or a new one,
    // which a restore continues.
    let thread_id = match &options.agent {
        AgentChoice::Builtin { .. } => Some(
            options
                .thread
                .unwrap_or_else(sheprd_chat_protocol::thread::ThreadId::random),
        ),
        AgentChoice::Command { .. } => None,
    };
    let server = thread_id.and_then(crate::thread_mcp::Server::of);
    let agent = agent(&options, &cwd, server.as_ref());
    let (inputs, receiver) = bounded::<Input>(INPUTS);
    // Where the chat listens is chosen before its agent starts, so the agent
    // can be told.
    let address = options.attach.then(crate::attach::address);
    let chat = address
        .as_ref()
        .and_then(|address| address.as_ref().ok())
        .map(|(_, id)| id.clone());
    let first = start(
        agent.start,
        &cwd,
        &inputs,
        HarnessId::default(),
        chat.as_ref(),
    )?;
    let agent_process = first.process.clone();
    // Stops the first agent on every early return; a switch stops it sooner.
    let owned_agent = agent_process.owned();
    // A claimed launch continues a session, which `--choose` refuses, so
    // its agent is never switched away from.
    if let Some(claimed) = &claimed {
        crate::restore::core::child_started(claimed, agent_process.id())?;
    }

    let mut notices = Vec::new();
    let history = agent.history.and_then(|history| match history {
        Ok(history) => Some(history),
        // The agent still has its session; only the view misses it.
        Err(error) => {
            notices.push((
                NoticeLevel::Warning,
                format!("Earlier messages cannot be shown: {error}"),
            ));
            None
        }
    });
    // Held to the end: dropping the server removes its socket.
    let (_server, endpoint) = match address {
        Some(address) => listen(address, inputs.clone(), &mut notices),
        None => (None, None),
    };
    let listens = endpoint.is_some();
    let target = Target::from_env();
    let thread = thread_id
        .zip(sheprd_chat_protocol::thread::threads_dir())
        .and_then(
            |(id, dir)| match crate::session::ThreadStart::read(dir, id) {
                Ok(start) => Some(start),
                Err(error) => {
                    notices.push((
                        NoticeLevel::Warning,
                        format!("This conversation cannot be kept as a thread: {error}"),
                    ));
                    None
                }
            },
        );
    let keeper = crate::restore::keeper(
        target.as_ref(),
        endpoint.as_ref(),
        &options.agent,
        adapter(&options),
        &cwd,
        agent_process.id(),
        thread_id,
    );
    let resumer = crate::resume::from_env(&options, adapter(&options), &cwd, thread_id);
    let session_resumer = resumer.clone();
    if let Some(keeper) = &keeper
        && let Err(error) = keeper.agent_started(agent_process.id())
    {
        notices.push((
            NoticeLevel::Warning,
            format!("This chat cannot be restored if it is lost: {error:#}"),
        ));
    }
    let reporter = match &target {
        Some(target) => Some(Reporter::start(
            target.clone(),
            agent.display.clone(),
            endpoint.map(|id| id.to_string()),
        )?),
        #[cfg(feature = "tmux")]
        None => Reporter::on_tmux(agent.display.clone(), endpoint.map(|id| id.to_string()))?,
        #[cfg(not(feature = "tmux"))]
        None => None,
    };
    let report = reporter.as_ref().map(Reporter::handle);
    let mut session_keeper = keeper.as_ref().map(|k| k.copy());
    let keep_history = keeper.is_some();
    let switch_keeper = keeper.as_ref().map(|k| k.copy());
    let adapter = adapter(&options);

    let harness = match &options.agent {
        AgentChoice::Builtin { agent, .. } => Some(*agent),
        AgentChoice::Command { .. } => None,
    };
    // A file of the user's that cannot be used says so in the chat, which
    // then uses the built-in text. A fixed Antigravity chat never annotates
    // prompts, so it skips loading prompt files; a `--choose` chat still loads
    // them in case the person switches to another agent.
    let prompts = if !options.choose && harness == Some(crate::agents::Agent::Antigravity) {
        crate::context::Prompts::default()
    } else {
        match crate::context::Prompts::dir() {
            Some(dir) => {
                let (prompts, problems) = crate::context::Prompts::load(&dir);
                notices.extend(
                    problems
                        .into_iter()
                        .map(|problem| (NoticeLevel::Warning, problem.to_string())),
                );
                prompts
            }
            None => crate::context::Prompts::default(),
        }
    };
    // The brief is read, and its file removed, before anything can fail to
    // start; one that cannot be read says so in the chat.
    let brief = options.brief_file.as_deref().and_then(|path| {
        crate::briefs::take(path, options.brief_from)
            .map_err(|error| {
                notices.push((
                    NoticeLevel::Warning,
                    format!("The brief this chat was started with was not sent: {error}"),
                ));
            })
            .ok()
    });
    let settings_dir = crate::known::dir();
    let picks_dir = settings_dir.clone();
    let settings = carried_settings(harness, options.load.is_some(), &mut notices);
    let config = Config {
        antigravity_auth_method: Some(antigravity_auth_method()),
        agent: agent.display,
        choices: harness.filter(|_| options.choose).map(choices),
        cwd: cwd.clone(),
        exit_with_agent: options.headless,
        gemini_auth_method: Some(gemini_auth_method()),
        given: agent.given,
        harness,
        history,
        known: settings_dir
            .as_deref()
            .map(crate::known::load)
            .unwrap_or_default(),
        listens,
        load: options.load,
        login_hint: agent.login_hint,
        pane: target.map(|target| target.pane),
        prompts,
        require_load: options.require_load,
        settings,
        sheprd: crate::context::sheprd_executable(),
        thread,
    };
    let launcher = inputs.clone();
    let session_thread = thread::Builder::new()
        .name("acp-session".to_owned())
        .spawn(move || {
            let mut session = Session::new(config, first.outbox, move |update| match update {
                Update::Harness(agent) => {
                    if let Some(keeper) = &mut session_keeper {
                        keeper.switched(agent);
                    }
                    crate::resume::with(session_resumer.as_ref(), |resumer| {
                        resumer.switched(agent);
                    });
                    if let Some(report) = &report {
                        report.rename(agent.display_name().to_owned());
                    }
                }
                Update::Started(pid) => {
                    if let Some(keeper) = &mut session_keeper {
                        keeper.agent_switched(pid);
                    }
                }
                Update::Opened(id) => {
                    if let Some(keeper) = &session_keeper {
                        keeper.opened(&id);
                    }
                    crate::resume::with(session_resumer.as_ref(), |resumer| resumer.opened(&id));
                }
                Update::Activity(activity) => {
                    if let Some(report) = &report {
                        report.report(activity);
                    }
                }
                Update::Prompt(prompt) => {
                    if let Some(report) = &report {
                        report.prompt(prompt);
                    }
                }
                Update::Settings(agent, settings) => {
                    if let Some(dir) = &settings_dir
                        && let Err(error) = crate::known::save(dir, agent, &settings)
                    {
                        eprintln!(
                            "sheprd-acp: could not keep {}'s settings: {error:#}",
                            agent.name()
                        );
                    }
                }
            });
            if keep_history && let Some(dir) = sheprd_chat_protocol::restore::restore_dir() {
                session.keep_history(dir.with_file_name("conversations"));
            }
            if let Some(brief) = &brief {
                session.link_delegation(&brief.text);
            }
            if let Some(dir) = crate::briefs::dir() {
                session.keep_briefs(dir);
            }
            if let Some(dir) = picks_dir {
                session.keep_picks(dir);
            }
            for (level, text) in notices {
                session.notice(level, text);
            }
            session.start();
            if let Some(brief) = brief {
                session.brief(brief);
            }
            let mut current = Some(first.process);
            let mut stopping = Vec::new();
            loop {
                let timer = match session.next_tick() {
                    Some(deadline) => crossbeam_channel::at(deadline),
                    None => crossbeam_channel::never(),
                };
                let input = crossbeam_channel::select! {
                    recv(receiver) -> input => match input {
                        Ok(input) => input,
                        Err(_) => break,
                    },
                    recv(timer) -> _ => {
                        session.tick(std::time::Instant::now());
                        continue;
                    }
                };
                let expected = crate::restore::expected_end(&input);
                match session.handle(input) {
                    Flow::Continue => {}
                    Flow::Exit => return (expected, current, stopping),
                    Flow::Launch {
                        agent,
                        harness,
                        load,
                    } => {
                        let held = switch_keeper
                            .as_ref()
                            .map_or(Ok(()), |keeper| keeper.hold_for_switch());
                        // The session left the old agent either way.
                        if let Some(old) = current.take() {
                            stopping.extend(stop_later(old));
                        }
                        if let Err(error) = held {
                            session.launch_failed(&error);
                            continue;
                        }
                        let next = builtin(agent, load.as_deref(), adapter, &cwd, server.as_ref());
                        match start(next.start, &cwd, &launcher, harness, chat.as_ref()) {
                            Ok(started) => {
                                let pid = started.process.id();
                                current = Some(started.process);
                                session.launched(started.outbox, pid, next.given);
                            }
                            Err(error) => session.launch_failed(&error),
                        }
                    }
                }
            }
            (false, current, stopping)
        })?;

    // Headless, the session ends when the agent does; otherwise when the view quits.
    let result = if options.headless {
        Ok(())
    } else {
        let result = crate::tui::run(&inputs);
        let _ = inputs.send(Input::Shutdown);
        result
    };
    let (ended, current, stopping) = session_thread.join().unwrap_or_default();
    let expected = ended && result.is_ok();
    // Only an intentional close or successful agent exit forgets continuity.
    // A failed view, failed agent, or panicked session keeps its record.
    if expected && let Some(keeper) = &keeper {
        keeper.ended();
    }
    if expected {
        crate::resume::with(resumer.as_ref(), crate::resume::Resumer::ended);
    }
    if let Some(current) = current {
        current.stop();
    }
    for handle in stopping {
        let _ = handle.join();
    }
    drop(owned_agent);
    if let Some(reporter) = reporter {
        reporter.close();
    }
    if options.headless {
        return headless_result(ended);
    }
    result
}

/// Headless, the chat program's exit status is the agent's. A failed or
/// crashed agent must not look like a chat that finished: core keeps a
/// recovery shell only for a program that did not end normally.
fn headless_result(expected: bool) -> anyhow::Result<()> {
    if expected {
        Ok(())
    } else {
        Err(crate::Error::AgentEnded.into())
    }
}

fn listen(
    address: crate::Result<(PathBuf, EndpointId)>,
    inputs: Sender<Input>,
    notices: &mut Vec<(NoticeLevel, String)>,
) -> (Option<crate::attach::Server>, Option<EndpointId>) {
    let listening = address.and_then(|(address, id)| {
        crate::attach::Server::listen(&address, inputs).map(|server| (server, id))
    });
    match listening {
        Ok((server, id)) => (Some(server), Some(id)),
        Err(error) => {
            notices.push((
                NoticeLevel::Warning,
                format!("Sheprd cannot attach: {:#}", anyhow::Error::from(error)),
            ));
            (None, None)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
