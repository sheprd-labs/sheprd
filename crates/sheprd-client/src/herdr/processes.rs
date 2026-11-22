//! Which of a pane's foreground processes run a given agent.

use super::snapshot::{Process, ProcessInfo};
use sheprd_chat_protocol::Agent;

mod identity;
pub use identity::{ProcessIdentity, local_identity};

/// The most processes matching an agent that are worth telling apart: past
/// these the pane runs something no single session explains.
const MAX_MATCHING_PROCESSES: usize = 16;

/// The foreground processes running `agent`, unless there are more than
/// [`MAX_MATCHING_PROCESSES`] to verify.
pub fn agent_processes(agent: Agent, info: &ProcessInfo) -> Option<Vec<&Process>> {
    let matching: Vec<_> = info
        .foreground_processes
        .iter()
        .filter(|process| is_agent(agent, &process.argv))
        .take(MAX_MATCHING_PROCESSES + 1)
        .collect();
    (matching.len() <= MAX_MATCHING_PROCESSES).then_some(matching)
}

/// The program an argument vector runs, without its directory.
pub fn program(argv: &[String]) -> Option<&str> {
    argv.first()?.rsplit('/').next()
}

/// Whether `argv` runs `agent` itself, directly or as a Node or Bun script,
/// rather than naming it as an argument to some other program.
pub fn is_agent(agent: Agent, argv: &[String]) -> bool {
    agent_args(agent, argv).is_some()
}

/// The arguments given to `agent` when `argv` runs it, natively or through
/// one of the Node or Bun launchers [`is_agent`] knows.
pub fn agent_args(agent: Agent, argv: &[String]) -> Option<&[String]> {
    let program = program(argv)?;
    if program == agent.name() {
        return Some(&argv[1..]);
    }
    if !matches!(program, "node" | "nodejs" | "bun") {
        return None;
    }
    let script = argv.get(1)?;
    let matches = match agent {
        Agent::Claude => {
            script.ends_with("/@anthropic-ai/claude-code/cli.js") || script.ends_with("/claude")
        }
        Agent::Codex => {
            script.ends_with("/@openai/codex/bin/codex.js") || script.ends_with("/codex")
        }
        _ => false,
    };
    matches.then_some(&argv[2..])
}

/// The one process running `agent` in a pane, among the matches
/// [`agent_processes`] finds, all in the foreground group. npm launchers
/// remain in the foreground alongside their native child: that child is
/// picked only when every other match is a verified launcher ancestor.
/// `identity` gives each process's parent, group and birth on its host.
pub fn agent_pid(
    agent: Agent,
    info: &ProcessInfo,
    identity: impl Fn(u32) -> Option<ProcessIdentity>,
) -> Option<u32> {
    let group = info.foreground_process_group_id?;
    let matching = agent_processes(agent, info)?;
    if matching.is_empty()
        || matching
            .iter()
            .any(|process| identity(process.pid).is_none_or(|stat| stat.group != group))
    {
        return None;
    }
    if matching.len() == 1 {
        return Some(matching[0].pid);
    }
    let mut native = matching
        .iter()
        .filter(|process| program(&process.argv) == Some(agent.name()));
    let pid = native.next()?.pid;
    if native.next().is_some() {
        return None;
    }
    for wrapper in matching.iter().filter(|process| process.pid != pid) {
        let mut child = pid;
        let ancestor = (0..matching.len()).any(|_| {
            let Some(stat) = identity(child) else {
                return false;
            };
            let Some(parent) = identity(stat.parent) else {
                return false;
            };
            if parent.started > stat.started
                || parent.group != group
                || !matching.iter().any(|process| process.pid == stat.parent)
            {
                return false;
            }
            child = stat.parent;
            child == wrapper.pid
        });
        if !ancestor {
            return None;
        }
    }
    Some(pid)
}

#[cfg(test)]
mod tests;
