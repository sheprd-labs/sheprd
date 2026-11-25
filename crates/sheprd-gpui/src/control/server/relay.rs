//! Requests another program relays to the app ([`Caller::via`]), refused
//! while the switch that allows that relay is off.
//!
//! [`Caller::via`]: crate::control::protocol::Caller::via

use crate::control::protocol::{Caller, ErrorCode, Request, Response, Via, tabs::TabOp};
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
        // What it may ask depends on the request: see [`lent`].
        Via::Lend { .. } => None,
    }
}

/// The answer refusing a request another computer relays through
/// `sheprd machine lend`, or `None` when the app serves it: only virtual
/// Mac work, and while lending is off only listing and releasing, so the
/// borrower learns why and can let go. Tart and other programs' VMs are
/// checked where a slot is taken, off the main thread.
pub(super) fn lent(request: &Request, cx: &App) -> Option<Response> {
    let Some(Via::Lend { .. }) = request.via() else {
        return None;
    };
    let op = match request {
        Request::Tab(call) if call.op.lendable() => &call.op,
        _ => {
            return Some(Response::error(
                ErrorCode::InvalidRequest,
                "Another computer may only ask for virtual Mac work here",
            ));
        }
    };
    if matches!(op, TabOp::MachineList | TabOp::MachineRelease { .. }) || lends(cx) {
        return None;
    }
    Some(Response::error(
        ErrorCode::Unsupported,
        "This computer does not lend its virtual Macs: `[machine] lend` is off in its config",
    ))
}

/// Whether `[machine] lend` is on, as the frontmost window read it; on
/// without a window, as it is by default.
pub(super) fn lends(cx: &App) -> bool {
    super::main_windows(cx)
        .first()
        .and_then(|handle| handle.read(cx).ok())
        .is_none_or(|view| view.config.machine.lend)
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
