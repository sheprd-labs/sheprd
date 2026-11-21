//! `remote push-status`: how the daemon's iPhone pushes leave it, asked on
//! the background executor. It only reads; the push relay is turned on in
//! Settings, never from here.
use crate::{
    control::protocol::{ErrorCode, Response, remote::Reply},
    remote::Supervisor,
};
use gpui::{App, Task};
use sheprd_remote_control::control::{self, Request};

/// The reply now while remote control is off, else the task asking the
/// daemon.
pub(super) fn serve(
    enabled: bool,
    push_relay: bool,
    cx: &mut App,
) -> Result<Task<Response>, Response> {
    if !enabled {
        return Err(reply(false, push_relay, None, None));
    }
    let Some(client) = Supervisor::client(cx) else {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "This Sheprd does not run remote control",
        ));
    };
    let request = client.request(Request::PushStatus);
    Ok(cx.spawn(async move |_| {
        // The error is display text: the reply carries it as the
        // command's message.
        let (status, error) = match request.await {
            Ok(control::Response::PushStatus(status)) => (Some(status), None),
            Ok(_) => (None, Some(crate::remote::Error::Unexpected.to_string())),
            Err(error) => (None, Some(error.to_string())),
        };
        reply(true, push_relay, status, error)
    }))
}

fn reply(
    enabled: bool,
    push_relay: bool,
    status: Option<control::push::PushStatus>,
    error: Option<String>,
) -> Response {
    Response::Remote {
        reply: Reply::PushStatus {
            enabled,
            push_relay,
            status,
            error,
        },
    }
}
