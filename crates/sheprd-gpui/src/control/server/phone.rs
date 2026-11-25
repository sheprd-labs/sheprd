//! `phone`: agents driving a paired phone's app. The daemon is asked on the
//! background executor; it waits for the phone, which refuses unless its
//! person allowed agents on this computer and the app is unlocked.
use super::{Watches, settings::answer_when_done, socket, window_for};
use crate::{
    control::protocol::{Call, ErrorCode, Response, phone::Ask},
    remote::Supervisor,
};
use gpui::App;
use sheprd_remote_control::control::{self, Request, phone::Answer};
use std::time::Duration;

const OFF: &str = "Remote control is off; turn it on in Settings > Remote control";

pub(super) fn serve(
    call: Call<Ask>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let Call { caller, op } = call;
    let enabled = match window_for(&caller, cx) {
        Ok(handle) => handle
            .read_with(cx, |view, _| view.config.remote.enabled)
            .unwrap_or_default(),
        Err(response) => return incoming.respond(response),
    };
    if !enabled {
        return incoming.respond(Response::error(ErrorCode::Failed, OFF));
    }
    let Some(client) = Supervisor::client(cx) else {
        return incoming.respond(Response::error(
            ErrorCode::Unsupported,
            "This Sheprd does not run remote control",
        ));
    };
    let request = client.request(Request::Phone(op));
    let task = cx.spawn(async move |_| match request.await {
        Ok(control::Response::Phone(answer)) => response(answer),
        Ok(_) => Response::error(ErrorCode::Failed, crate::remote::Error::Unexpected.to_string()),
        // An older daemon closes without answering.
        Err(control::Error::NoAnswer) => Response::error(
            ErrorCode::Unsupported,
            "The remote control daemon cannot drive phones; restart it with `sheprd remote restart`",
        ),
        Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
    });
    let wait = Duration::from_secs(crate::control::protocol::phone::WAIT_SECONDS);
    answer_when_done(task, wait, incoming, watches, cx);
}

/// The daemon's answer as the command's: what it could not do as the
/// error codes every command shares, so exit codes follow.
pub(super) fn response(answer: Answer) -> Response {
    let (code, message) = match answer {
        Answer::NotFound => (
            ErrorCode::NotFound,
            "No such pane or workspace on this computer".to_owned(),
        ),
        Answer::Invalid => (
            ErrorCode::InvalidRequest,
            "That pane, workspace, or note is empty or too long".to_owned(),
        ),
        Answer::Ambiguous => (
            ErrorCode::InvalidRequest,
            "Two phones have that name; name it by its ID from `sheprd phone`".to_owned(),
        ),
        Answer::NoAnswer { phone } => (
            ErrorCode::Timeout,
            format!(
                "{} did not answer; it may have gone to the background",
                phone.name
            ),
        ),
        answer => return Response::Phone { answer },
    };
    Response::error(code, message)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
