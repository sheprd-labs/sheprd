//! Requests another program relays to the app ([`Caller::via`]), refused
//! while the switch that allows that relay is off.
//!
//! [`Caller::via`]: crate::control::protocol::Caller::via

use crate::control::protocol::{Caller, ErrorCode, Response, Via};
use crate::{SheprdWindow, i18n::fl, window::Flash};
use gpui::{App, Context};

/// The answer refusing `via`, or `None` when the app serves it.
pub(super) fn refusal(via: &Via, cx: &App) -> Option<Response> {
    if !via.is_valid() {
        return Some(Response::error(
            ErrorCode::InvalidRequest,
            "The relayed client name must be printable and short",
        ));
    }
    match via {
        Via::Mcp { .. } => mcp(cx),
    }
}

/// Says who asked, once a relayed request opened a dialog: the dialog names
/// only what it would do, and the person should know a program, not they,
/// started it. Direct requests come from the person's own terminal.
pub(super) fn announce(view: &mut SheprdWindow, caller: &Caller, cx: &mut Context<SheprdWindow>) {
    let Some(Via::Mcp { client }) = &caller.via else {
        return;
    };
    let text = match client {
        Some(client) => fl!("mcp-dialog-requested", client = client.clone()),
        None => fl!("mcp-dialog-requested-unnamed"),
    };
    view.show_flash(Flash::hint(text), cx);
}

/// Every window reads the same config file, so the frontmost one's copy
/// says whether the switch is on. Without a window nothing is read and the
/// request is refused: the switch is off unless the app can see it is on.
#[cfg(feature = "mcp")]
fn mcp(cx: &App) -> Option<Response> {
    let enabled = super::main_windows(cx)
        .first()
        .and_then(|handle| handle.read(cx).ok())
        .map(|view| view.config.features.mcp);
    match enabled {
        Some(true) => None,
        Some(false) => Some(Response::error(
            ErrorCode::Unsupported,
            "The MCP server is off; turn on MCP server in Sheprd's Settings > General",
        )),
        None => Some(Response::error(
            ErrorCode::NoWindow,
            "Sheprd has no window open, so it cannot tell whether the MCP server is on",
        )),
    }
}

#[cfg(not(feature = "mcp"))]
fn mcp(_: &App) -> Option<Response> {
    Some(Response::error(
        ErrorCode::Unsupported,
        "This build of Sheprd has no MCP server",
    ))
}

#[cfg(test)]
mod tests;
