//! `fleet status`, `fleet open`, `fleet remove`, `fleet remove-host`,
//! `fleet scrub`, `fleet invite`, and `fleet join`.
//! The daemon is asked on the background executor. Removing opens
//! Settings > Fleet asking, where the user picks why and confirms; nothing
//! is signed from here.
use super::{Watches, in_window, settings::answer_when_done, socket, window_for};
use crate::{
    control::protocol::{
        Call, ErrorCode, Response,
        fleet::{FleetOp, Reply},
    },
    remote::Supervisor,
    settings_window::FleetPending,
};
use gpui::App;
use sheprd_remote_control::control::{
    Request,
    fleet::{
        View,
        sync::{Answer, Ask},
    },
};
use std::time::Duration;

pub(super) fn serve(
    call: Call<FleetOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let wait = Duration::from_secs(call.op.wait_seconds());
    let Call { caller, op } = call;
    if let Err(response) = window_for(&caller, cx) {
        return incoming.respond(response);
    }
    // A removal is resolved against the daemon's view as it is now, not
    // the one Settings last read, so a member or host just added is found.
    let target = match op {
        FleetOp::Status => None,
        FleetOp::Open => {
            return incoming.respond(open(&caller, FleetPending::Nothing, cx));
        }
        FleetOp::Remove { member } => Some(Target::Member(member)),
        FleetOp::RemoveHost { host } => Some(Target::Host(host)),
        // Machines are this Sheprd's own: no daemon to ask.
        FleetOp::Scrub { machine } => {
            let machines = crate::fleet::Fleet::machines(cx);
            let named = machines
                .iter()
                .map(|(id, machine)| (id.as_str(), machine.label.as_str()));
            let response = match pick(named, &machine, "machine") {
                Ok(id) => open(&caller, FleetPending::Scrub(id), cx),
                Err(response) => response,
            };
            return incoming.respond(response);
        }
        // So are the machines asked to join, and the fleets they offer.
        FleetOp::Invite { machine } => {
            let member = crate::fleet::Fleet::member_of(cx).is_some();
            let outside: Vec<(String, String)> = crate::fleet::Fleet::machines(cx)
                .into_iter()
                .filter(|(id, _)| {
                    crate::fleet::Fleet::standing(cx, id) == crate::fleet::Standing::Outside
                })
                .map(|(id, machine)| (id, machine.label))
                .collect();
            let response = match invite_pending(member, &outside, machine.as_deref()) {
                Ok(pending) => open(&caller, pending, cx),
                Err(response) => response,
            };
            return incoming.respond(response);
        }
        FleetOp::Join { machine } => {
            let in_fleet = crate::fleet::Fleet::view(cx).is_some();
            let offers: Vec<(String, String)> = crate::fleet::Fleet::offers(cx)
                .into_iter()
                .flat_map(|offered| offered.through)
                .map(|(id, machine)| (id, machine.label))
                .collect();
            let response = match join_pending(in_fleet, &offers, machine.as_deref()) {
                Ok(pending) => open(&caller, pending, cx),
                Err(response) => response,
            };
            return incoming.respond(response);
        }
    };
    let Some(client) = Supervisor::client(cx) else {
        return incoming.respond(Response::error(
            ErrorCode::Unsupported,
            "This Sheprd does not run remote control",
        ));
    };
    let request = client.request(Request::Fleet(Box::new(Ask::Status)));
    let task = cx.spawn(async move |cx| {
        let (view, error) = match request.await {
            Ok(sheprd_remote_control::control::Response::Fleet(answer)) => match *answer {
                Answer::Status { view, .. } => (view, None),
                _ => (None, Some(crate::remote::Error::Unexpected.to_string())),
            },
            Ok(sheprd_remote_control::control::Response::Error { message, .. }) => {
                (None, Some(message))
            }
            Ok(_) => (None, Some(crate::remote::Error::Unexpected.to_string())),
            Err(error) => (None, Some(error.to_string())),
        };
        let Some(target) = target else {
            return Response::Fleet {
                reply: Reply::Status { view, error },
            };
        };
        match pending(view.as_deref(), error, target) {
            Ok(pending) => cx.update(|cx| open(&caller, pending, cx)),
            Err(response) => response,
        }
    });
    answer_when_done(task, wait, incoming, watches, cx);
}

/// What `fleet remove` or `fleet remove-host` names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Member(String),
    Host(String),
}

/// The question Settings > Fleet opens asking for `target`, resolved
/// against `view`, the daemon's answer, or why not: `error` when the
/// daemon could not be read.
pub(super) fn pending(
    view: Option<&View>,
    error: Option<String>,
    target: Target,
) -> Result<FleetPending, Response> {
    if let Some(error) = error {
        return Err(Response::error(
            ErrorCode::Failed,
            format!("Could not read the fleet: {error}"),
        ));
    }
    match target {
        Target::Member(member) => pick_member(view, &member).map(FleetPending::Remove),
        Target::Host(host) => pick_host(view, &host).map(FleetPending::RemoveHost),
    }
}

/// What `fleet invite` asks: this computer must be a member, and the
/// machine named, or any with none named, one of `outside`, the machines
/// this Sheprd reaches that are in no fleet (ID and name).
pub(super) fn invite_pending(
    member: bool,
    outside: &[(String, String)],
    machine: Option<&str>,
) -> Result<FleetPending, Response> {
    if !member {
        return Err(not_in_fleet());
    }
    let named = outside
        .iter()
        .map(|(id, name)| (id.as_str(), name.as_str()));
    match machine {
        Some(machine) => pick(named, machine, "computer outside a fleet").map(FleetPending::Invite),
        None if outside.is_empty() => Err(Response::error(
            ErrorCode::NotFound,
            "No computer this Sheprd reaches is outside a fleet",
        )),
        None => Ok(FleetPending::InviteAll),
    }
}

/// What `fleet join` asks: this computer must be in no fleet, and the
/// machine named, or any with none named, one of `offers`, the machines
/// this Sheprd reaches that are members of a fleet (ID and name).
pub(super) fn join_pending(
    in_fleet: bool,
    offers: &[(String, String)],
    machine: Option<&str>,
) -> Result<FleetPending, Response> {
    if in_fleet {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            "This computer is in a fleet already; leave it in Settings > Fleet first",
        ));
    }
    let named = offers.iter().map(|(id, name)| (id.as_str(), name.as_str()));
    match machine {
        Some(machine) => {
            pick(named, machine, "computer in a fleet").map(|id| FleetPending::Join(Some(id)))
        }
        None if offers.is_empty() => Err(Response::error(
            ErrorCode::NotFound,
            "No computer this Sheprd reaches is a member of a fleet; open Settings > Fleet to look again",
        )),
        None => Ok(FleetPending::Join(None)),
    }
}

/// Opens Settings > Fleet in the caller's window, asking `pending`.
fn open(
    caller: &crate::control::protocol::Caller,
    pending: FleetPending,
    cx: &mut App,
) -> Response {
    let asking = !matches!(pending, FleetPending::Nothing);
    in_window(caller, cx, move |_, _, cx| {
        crate::settings_window::open_fleet(cx.entity().downgrade(), pending, cx);
        if asking {
            Response::done("Asking in Sheprd's Settings > Fleet; the user confirms there")
        } else {
            Response::done("Showing Settings > Fleet")
        }
    })
}

/// The member `needle` names: by ID, else the one member of that name.
pub(super) fn pick_member(view: Option<&View>, needle: &str) -> Result<String, Response> {
    let view = view.ok_or_else(not_in_fleet)?;
    pick(
        view.members
            .iter()
            .map(|member| (member.id.as_str(), member.name.as_str())),
        needle,
        "member",
    )
}

/// The host `needle` names: by ID, else the one host of that name.
pub(super) fn pick_host(view: Option<&View>, needle: &str) -> Result<String, Response> {
    let view = view.ok_or_else(not_in_fleet)?;
    pick(
        view.hosts
            .iter()
            .map(|host| (host.id.as_str(), host.name.as_str())),
        needle,
        "host",
    )
}

fn pick<'a>(
    items: impl Iterator<Item = (&'a str, &'a str)> + Clone,
    needle: &str,
    what: &str,
) -> Result<String, Response> {
    if let Some((id, _)) = items.clone().find(|(id, _)| *id == needle) {
        return Ok(id.to_owned());
    }
    let mut named = items.filter(|(_, name)| *name == needle);
    match (named.next(), named.next()) {
        (Some((id, _)), None) => Ok(id.to_owned()),
        (Some(_), Some(_)) => Err(Response::error(
            ErrorCode::InvalidRequest,
            format!("Several {what}s are named {needle}; name one by its ID from `fleet status`"),
        )),
        (None, _) => Err(Response::error(
            ErrorCode::NotFound,
            format!("No {what} is named {needle}"),
        )),
    }
}

fn not_in_fleet() -> Response {
    Response::error(
        ErrorCode::NotFound,
        "This computer is in no fleet; create one in Settings > Fleet",
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
