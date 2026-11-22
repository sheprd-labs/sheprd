//! Which terminal agents' conversations Sheprd reads from the transcripts
//! their own CLIs keep, rather than through a chat program: the desktop's
//! `render_existing` and the remote control daemon's chats for phones both
//! ask here, and `sheprd-acp observe` reads what these name. Another agent
//! joins with one entry in [`observable`], a reader for its log in
//! `sheprd-acp`, and, when its launch can name a session, a check in
//! [`launch_agrees`].

pub mod codex;

use sheprd_chat_protocol::Agent;

/// The agent named `name`, when its running conversation can be observed:
/// its CLI reports a session id whose transcript the observer can read.
pub fn observable(name: &str) -> Option<Agent> {
    match name.parse() {
        Ok(agent @ (Agent::Codex | Agent::Claude)) => Some(agent),
        _ => None,
    }
}

/// Whether `agent`'s launch arguments `args` (after its program) agree that
/// it runs `session`, as Herdr reports. Only a conflict refuses: argv
/// cannot attest to a session the agent switched to since.
pub fn launch_agrees(agent: Agent, args: &[String], session: &str) -> bool {
    match agent {
        Agent::Codex => codex::session_matches(args, session),
        _ => true,
    }
}
