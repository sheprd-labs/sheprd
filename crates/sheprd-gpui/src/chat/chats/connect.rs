//! The connector the app uses for each chat it attaches.

use crate::chat::{
    Error, Result, Target,
    link::{Dial, Link},
};
use sheprd_chat_protocol::{ClientMessage, Cursor};

/// The connector the app uses: the program's endpoint by the id its pane
/// reported, a socket in this user's private directory or a named pipe, or
/// for a chat on an SSH host, a stream of the host's multiplexer, or
/// `herdr-acp connect` run there over `ssh` where it has none. An
/// existing agent on this machine is observed by `program`, the same
/// `herdr-acp` new chats start with.
pub(crate) fn connect(
    target: &Target,
    program: &crate::chat::Program,
    since: Option<Cursor>,
    resend: Vec<ClientMessage>,
) -> Result<Link> {
    if let Some(binding) = &target.existing {
        let args = [
            "observe".to_owned(),
            binding.agent.name().to_owned(),
            binding.session_id.clone(),
            binding.pid.to_string(),
            binding.started_ticks.to_string(),
            "--pane".to_owned(),
            target.pane_id.clone(),
        ];
        let command = match &target.route {
            crate::chat::Route::Remote(host) => {
                let args = args
                    .iter()
                    .map(|arg| sheprd_client::shell_quote(arg))
                    .collect::<Vec<_>>()
                    .join(" ");
                return Link::start(
                    Dial::Remote(host.clone(), crate::chat::remote_program_script(&args)),
                    since,
                    resend,
                );
            }
            crate::chat::Route::Local => {
                // Unlike a missing socket directory, this is not permanent:
                // the lookup started this tick may still be running.
                let mut command = sheprd_client::new_command(program.found()?);
                command.args(args);
                command
            }
            crate::chat::Route::Ssh(host) => {
                let args = args
                    .iter()
                    .map(|arg| sheprd_client::shell_quote(arg))
                    .collect::<Vec<_>>()
                    .join(" ");
                return Link::start(
                    Dial::Ssh(host.clone(), crate::chat::remote_program_script(&args)),
                    since,
                    resend,
                );
            }
        };
        return Link::start(Dial::Process(command), since, resend);
    }
    let dial = match &target.route {
        crate::chat::Route::Remote(host) => Dial::Remote(
            host.clone(),
            crate::chat::remote_program_script(&format!("connect {}", target.endpoint)),
        ),
        crate::chat::Route::Local => Dial::Socket(
            sheprd_chat_protocol::endpoint_address(&target.endpoint).ok_or(Error::NoEndpointDir)?,
        ),
        crate::chat::Route::Ssh(host) => Dial::Mux(host.clone(), target.endpoint.clone()),
    };
    Link::start(dial, since, resend)
}
