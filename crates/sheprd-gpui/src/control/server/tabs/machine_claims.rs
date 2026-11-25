//! Pane-owned reservations, serialized by the app thread across all windows.
use crate::control::{
    protocol::{Caller, ErrorCode, Response, tabs::TabOp},
    server::socket,
};
use gpui::App;

#[cfg(not(target_os = "macos"))]
pub(super) fn serve(_: Caller, _: TabOp, incoming: socket::Incoming, _: &mut App) {
    incoming.respond(Response::error(
        ErrorCode::Unsupported,
        "Virtual Macs need an Apple silicon Mac",
    ));
}

#[cfg(target_os = "macos")]
pub(super) fn serve(caller: Caller, op: TabOp, incoming: socket::Incoming, cx: &mut App) {
    use crate::machine;
    let (name, release) = match op {
        TabOp::MachineClaim { name } => (name, false),
        TabOp::MachineRelease { name } => (name, true),
        _ => return,
    };
    if owner(&caller).is_none() {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "Reserve from a Herdr pane or core terminal",
        ));
    }
    let name = match machine::Name::try_from(name) {
        Ok(name) => name,
        Err(error) => {
            return incoming.respond(Response::error(
                ErrorCode::InvalidRequest,
                error.to_string(),
            ));
        }
    };
    let read = cx
        .background_executor()
        .spawn(async { (machine::list(), machine::lend::Survey::read()) });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let (listed, survey) = read.await;
        let response = cx.update(|cx| match listed {
            Ok(listed) if listed.iter().any(|item| item.name == name) => {
                if !release && let Some(refusal) = not_lending(&caller, survey, cx) {
                    return refusal;
                }
                let capacity = survey.capacity(machine::capacity().in_use);
                cx.default_global::<Reservations>().change(
                    &caller,
                    name.as_str(),
                    release,
                    Instant::now(),
                    machine::check_capacity_in(&name, capacity),
                )
            }
            Ok(_) => Response::error(ErrorCode::NotFound, "No such virtual Mac"),
            Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
        });
        incoming.respond(response);
    })
    .detach();
}

/// The refusal of another computer's claim or start while this Mac does
/// not lend, from what `survey` read; `None` for local callers.
#[cfg(target_os = "macos")]
pub(super) fn not_lending(
    caller: &Caller,
    survey: crate::machine::lend::Survey,
    cx: &App,
) -> Option<Response> {
    caller.via.as_ref()?.lend()?;
    let sheprd = crate::machine::capacity().in_use;
    let reason = survey
        .lending(crate::control::server::relay::lends(cx), sheprd)
        .reason()?;
    Some(Response::error(
        ErrorCode::Unsupported,
        format!("This computer does not lend its virtual Macs: {reason}"),
    ))
}

/// Normalize on a worker before checking the app-thread reservation and
/// dispatching a filesystem mutation. Never check the caller's raw spelling.
#[cfg(target_os = "macos")]
pub(super) fn guarded(
    caller: Caller,
    name: String,
    incoming: socket::Incoming,
    cx: &mut App,
    run: impl FnOnce(&Caller, String, socket::Incoming, &mut App) + 'static,
) {
    let read = cx.background_executor().spawn(async move {
        let name = crate::machine::Name::try_from(name)?;
        crate::machine::resolve_name(&name)
    });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let resolved = read.await;
        cx.update(|cx| match resolved {
            Ok(name) => {
                if let Some(response) = check(&caller, name.as_str(), cx) {
                    incoming.respond(response);
                } else {
                    run(&caller, name.to_string(), incoming, cx);
                }
            }
            Err(error) => {
                let code = match &error {
                    crate::machine::Error::InvalidName => ErrorCode::InvalidRequest,
                    crate::machine::Error::NotInstalled(_) => ErrorCode::NotFound,
                    crate::machine::Error::Read { source, .. }
                        if source.kind() == std::io::ErrorKind::NotFound =>
                    {
                        ErrorCode::NotFound
                    }
                    _ => ErrorCode::Failed,
                };
                incoming.respond(Response::error(code, error.to_string()));
            }
        });
    })
    .detach();
}

#[cfg(not(target_os = "macos"))]
pub(super) fn guarded(
    _: Caller,
    _: String,
    incoming: socket::Incoming,
    _: &mut App,
    _: impl FnOnce(&Caller, String, socket::Incoming, &mut App) + 'static,
) {
    incoming.respond(Response::error(
        ErrorCode::Unsupported,
        "Virtual Macs need an Apple silicon Mac",
    ));
}

#[cfg(any(target_os = "macos", test))]
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[cfg(any(target_os = "macos", test))]
const IDLE: Duration = Duration::from_secs(15 * 60);

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, PartialEq, Eq)]
struct Owner {
    daemon: Option<String>,
    pane: Option<String>,
    terminal: Option<String>,
    /// The computer and the pane there that borrowed the machine through
    /// `machine lend`; never set with the fields above.
    borrower: Option<String>,
}

#[cfg(any(target_os = "macos", test))]
fn owner(caller: &Caller) -> Option<Owner> {
    if let Some((borrower, owner)) = caller.via.as_ref().and_then(|via| via.lend()) {
        return Some(Owner {
            daemon: None,
            pane: None,
            terminal: None,
            borrower: Some(format!("{borrower} ({owner})")),
        });
    }
    if caller.pane_id.is_none() && caller.terminal_id.is_none() {
        return None;
    }
    Some(Owner {
        daemon: caller.daemon_socket.clone(),
        pane: caller.pane_id.clone(),
        terminal: caller.terminal_id.clone(),
        borrower: None,
    })
}

#[cfg(any(target_os = "macos", test))]
struct Reservation {
    owner: Owner,
    until: Instant,
}

#[cfg(any(target_os = "macos", test))]
impl Reservation {
    fn describe(&self, now: Instant) -> String {
        if let Some(borrower) = &self.owner.borrower {
            return format!(
                "busy: borrowed by {borrower} ({}s remaining)",
                self.until.saturating_duration_since(now).as_secs()
            );
        }
        format!(
            "busy: pane {}, terminal {} ({}s remaining)",
            self.owner.pane.as_deref().unwrap_or("-"),
            self.owner.terminal.as_deref().unwrap_or("-"),
            self.until.saturating_duration_since(now).as_secs()
        )
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct Reservations(BTreeMap<String, Reservation>);
#[cfg(any(target_os = "macos", test))]
impl gpui::Global for Reservations {}

#[cfg(any(target_os = "macos", test))]
impl Reservations {
    fn prune(&mut self, now: Instant) {
        self.0.retain(|_, lease| lease.until > now);
    }

    fn change(
        &mut self,
        caller: &Caller,
        name: &str,
        release: bool,
        now: Instant,
        can_start: crate::machine::Result<()>,
    ) -> Response {
        self.prune(now);
        let Some(owner) = owner(caller) else {
            return Response::error(ErrorCode::InvalidRequest, "Reserve from a pane or terminal");
        };
        if let Some(lease) = self.0.get(name)
            && lease.owner != owner
        {
            return Response::error(
                ErrorCode::Busy,
                format!("{name} is {}", lease.describe(now)),
            );
        }
        // Renewing an owned lease or releasing it needs no running slot.
        // Expired leases were pruned above and must pass admission again.
        if !release
            && !self.0.contains_key(name)
            && let Err(error) = can_start
        {
            return Response::error(ErrorCode::Busy, error.to_string());
        }
        if release {
            self.0.remove(name);
            Response::done(format!("Released {name}"))
        } else {
            // Bound even malformed clients; installed machine count is normally tiny.
            if self.0.len() >= 64 && !self.0.contains_key(name) {
                return Response::error(ErrorCode::Busy, "Too many machine reservations");
            }
            self.0.insert(
                name.to_owned(),
                Reservation {
                    owner,
                    until: now + IDLE,
                },
            );
            Response::done(format!(
                "Reserved {name} for this pane for 15 idle minutes; release when finished"
            ))
        }
    }

    fn check(&mut self, caller: &Caller, name: &str, now: Instant) -> Option<Response> {
        self.prune(now);
        let lease = self.0.get_mut(name)?;
        if owner(caller).as_ref() != Some(&lease.owner) {
            return Some(Response::error(
                ErrorCode::Busy,
                format!("{name} is {}", lease.describe(now)),
            ));
        }
        // Guest commands take at most 600s, less than the idle lease.
        lease.until = now + IDLE;
        None
    }
}

#[cfg(target_os = "macos")]
pub(super) fn check(caller: &Caller, name: &str, cx: &mut App) -> Option<Response> {
    cx.default_global::<Reservations>()
        .check(caller, name, Instant::now())
}

#[cfg(target_os = "macos")]
pub(super) fn describe(name: &str, cx: &mut App) -> Option<String> {
    let now = Instant::now();
    let reservations = cx.default_global::<Reservations>();
    reservations.prune(now);
    reservations.0.get(name).map(|lease| lease.describe(now))
}

#[cfg(test)]
mod tests;
