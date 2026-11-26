//! `host pause` and `host resume`: a saved device's pause, answered once it
//! is saved, or with the reason it could not be.
use super::{Watches, named, socket, window_for};
use crate::{
    SheprdWindow,
    control::protocol::{Caller, ErrorCode, Response},
    menu::devices::{PauseOutcome, PauseTicket},
};
use gpui::App;
use std::time::Duration;

/// Longer than the `herdr machine` command may take.
const WAIT: Duration = Duration::from_secs(30);

pub(super) fn run(
    caller: Caller,
    name: &str,
    paused: bool,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let handle = match window_for(&caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let started = handle
        .update(cx, |view, _, cx| start(view, name, paused, cx))
        .unwrap_or_else(|_| Err(Response::error(ErrorCode::NoWindow, "The window closed")));
    let (id, label, ticket) = match started {
        Ok(Started::Waiting { id, label, ticket }) => (id, label, ticket),
        Ok(Started::Already(response)) | Err(response) => return incoming.respond(response),
    };
    watches.add(
        incoming,
        WAIT,
        move |cx| {
            handle
                .read_with(cx, |view, _| {
                    view.pause_outcome(&id, ticket)
                        .map(|outcome| answer(&label, paused, outcome))
                })
                .unwrap_or_else(|_| Some(Response::error(ErrorCode::NoWindow, "The window closed")))
        },
        Response::error(
            ErrorCode::Timeout,
            "The device's pause was not saved in time",
        ),
        cx,
    );
}

enum Started {
    /// Nothing to change.
    Already(Response),
    Waiting {
        id: String,
        label: String,
        ticket: PauseTicket,
    },
}

/// Asks the window to pause or resume the device named by ID or label.
fn start(
    view: &mut SheprdWindow,
    name: &str,
    paused: bool,
    cx: &mut gpui::Context<SheprdWindow>,
) -> Result<Started, Response> {
    let endpoint = named(view, name)?;
    let (id, label) = (endpoint.id.clone(), endpoint.label.clone());
    if !view.device_pausable(&id) {
        return Err(Response::error(
            ErrorCode::Unsupported,
            format!("{label} cannot be paused: only saved SSH and cloud devices can"),
        ));
    }
    // What the latest request asked, not only what the catalog says yet, so
    // a resume right after a pause is never skipped.
    if view.device_paused(&id) == paused {
        let state = if paused {
            "already paused"
        } else {
            "not paused"
        };
        return Ok(Started::Already(Response::done(format!(
            "{label} is {state}"
        ))));
    }
    let ticket = view
        .set_device_paused(&id, paused, cx)
        .ok_or_else(|| Response::error(ErrorCode::Failed, format!("{label} cannot be paused")))?;
    Ok(Started::Waiting { id, label, ticket })
}

fn answer(label: &str, paused: bool, outcome: PauseOutcome) -> Response {
    match outcome {
        PauseOutcome::Saved if paused => {
            Response::done(format!("Paused {label}; it stays listed but disconnects"))
        }
        PauseOutcome::Saved => Response::done(format!("Resumed {label}; it reconnects shortly")),
        PauseOutcome::Failed(error) => Response::error(ErrorCode::Failed, error),
    }
}
