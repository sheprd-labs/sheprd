//! `machine list`, `set`, `suspend`, and `delete`. Deleting asks in Settings:
//! it destroys what is on the machine, so the user confirms there.

use super::*;
#[cfg(target_os = "macos")]
use crate::{
    control::{
        protocol::tabs::{MachineImageInfo, MachineInfo, MachineState},
        server::window_for,
    },
    i18n::Localize as _,
    machine::settings_request::Request,
    machine::{self, Limits, Listing, Name, Settings, Status, image},
    window::main_window::MainWindow,
};

#[cfg(target_os = "macos")]
use crate::settings_window::MachineDeletion as Asked;

/// What `machine set` changes: CPUs, memory in GB, and the display.
type Changes = (Option<u16>, Option<u16>, Option<String>);

#[cfg(not(target_os = "macos"))]
fn unsupported(incoming: socket::Incoming) {
    incoming.respond(Response::error(
        ErrorCode::Unsupported,
        "Virtual Macs need an Apple silicon Mac",
    ));
}

#[cfg(not(target_os = "macos"))]
pub(super) fn list(incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn set(_: String, _: Changes, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn start(_: &Caller, _: String, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn suspend(_: String, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn delete(_: &Caller, _: String, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn new(
    _: &Caller,
    _: Option<String>,
    _: Option<String>,
    incoming: socket::Incoming,
    _: &mut App,
) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn image_list(incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn image_save(_: String, _: String, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn image_delete(_: &Caller, _: String, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(target_os = "macos")]
fn named(name: String, incoming: socket::Incoming) -> Option<(Name, socket::Incoming)> {
    match Name::try_from(name) {
        Ok(name) => Some((name, incoming)),
        Err(error) => {
            incoming.respond(Response::error(
                ErrorCode::InvalidRequest,
                error.to_string(),
            ));
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn info(listing: Listing, cx: &mut App) -> MachineInfo {
    let capabilities = listing.capabilities();
    let state = match (machine::status(&listing.name), listing.saved) {
        (Status::Running, _) => MachineState::Running,
        (Status::Starting, _) => MachineState::Starting,
        (Status::Saving, _) => MachineState::Saving,
        (Status::Failed(_), _) => MachineState::Failed,
        (Status::Stopped, true) => MachineState::Suspended,
        (Status::Stopped, false) => MachineState::Stopped,
    };
    MachineInfo {
        reservation: machine_claims::describe(listing.name.as_str(), cx),
        name: listing.name.to_string(),
        state,
        os: listing.guest.os,
        arch: listing.guest.arch,
        image: listing.guest.image_label(),
        capabilities,
        cpus: listing.settings.cpus,
        memory_gib: listing.settings.memory_gib,
        display: listing.settings.display.to_string(),
        disk_bytes: listing.disk_bytes,
    }
}

#[cfg(target_os = "macos")]
fn failed(error: &machine::Error) -> Response {
    Response::error(ErrorCode::Failed, error.to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn list(incoming: socket::Incoming, cx: &mut App) {
    let read = cx
        .background_executor()
        .spawn(async { (machine::list(), machine::lend::Survey::read()) });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let (listed, survey) = read.await;
        let response = cx.update(|cx| match listed {
            Ok(listings) => Response::Tab {
                reply: Reply::Machines {
                    capacity: Some(survey.capacity(machine::capacity().in_use)),
                    lending: Some(survey.lending(
                        crate::control::server::relay::lends(cx),
                        machine::capacity().in_use,
                    )),
                    machines: listings
                        .into_iter()
                        .map(|listing| info(listing, cx))
                        .collect(),
                },
            },
            Err(error) => failed(&error),
        });
        incoming.respond(response);
    })
    .detach();
}

/// `settings` with `changes`, checked against this Mac.
#[cfg(target_os = "macos")]
fn changed(settings: Settings, changes: Changes) -> machine::Result<Settings> {
    let (cpus, memory_gib, display) = changes;
    let settings = Settings {
        cpus: cpus.unwrap_or(settings.cpus),
        memory_gib: memory_gib.unwrap_or(settings.memory_gib),
        display: display.map_or(Ok(settings.display), |display| display.parse())?,
    };
    Limits::host().check(&settings)?;
    Ok(settings)
}

#[cfg(target_os = "macos")]
pub(super) fn set(name: String, changes: Changes, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let write = cx.background_executor().spawn(async move {
        machine::update_settings(&name, |settings| changed(settings, changes))?;
        Ok::<_, machine::Error>(name)
    });
    cx.spawn(async move |_: &mut gpui::AsyncApp| {
        incoming.respond(match write.await {
            Ok(name) => Response::done(format!("{name} takes these when it next boots")),
            Err(error @ machine::Error::NotInstalled(_)) => {
                Response::error(ErrorCode::NotFound, error.to_string())
            }
            Err(
                error @ (machine::Error::Cpus { .. }
                | machine::Error::Memory { .. }
                | machine::Error::InvalidDisplay),
            ) => Response::error(ErrorCode::InvalidRequest, error.localize()),
            Err(error) => failed(&error),
        });
    })
    .detach();
}

/// Starts machine `name` without a tab, as a borrowed machine does: no one
/// is at its screen. Answers once it is on its way; `machine agent` tells
/// when it is ready.
#[cfg(target_os = "macos")]
pub(super) fn start(caller: &Caller, name: String, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let caller = caller.clone();
    let read = cx
        .background_executor()
        .spawn(async { machine::lend::Survey::read() });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let survey = read.await;
        let response = cx.update(|cx| {
            if let Some(refusal) = machine_claims::not_lending(&caller, survey, cx) {
                return refusal;
            }
            match machine::status(&name) {
                Status::Running | Status::Starting => {
                    return Response::done(format!("{name} is running")); // i18n: data
                }
                Status::Saving => {
                    return Response::error(
                        ErrorCode::Busy,
                        format!("{name} is saving; start it once it stops"), // i18n: data
                    );
                }
                Status::Stopped | Status::Failed(_) => {}
            }
            let capacity = survey.capacity(machine::capacity().in_use);
            if let Err(error) = machine::check_capacity_in(&name, capacity) {
                return Response::error(ErrorCode::Busy, error.to_string());
            }
            machine::start(&name, cx);
            match machine::status(&name) {
                Status::Failed(reason) => Response::error(ErrorCode::Failed, reason),
                _ => Response::done(format!("Starting {name}")), // i18n: data
            }
        });
        incoming.respond(response);
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(super) fn suspend(name: String, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    if machine::status(&name) != Status::Running {
        return incoming.respond(Response::error(
            ErrorCode::Failed,
            format!("{name} is not running"), // i18n: data
        ));
    }
    match machine::suspend(&name, cx) {
        Ok(()) => incoming.respond(Response::done(format!("Saving {name}"))),
        Err(error @ machine::Error::UnsupportedCapability { .. }) => {
            incoming.respond(Response::error(ErrorCode::Unsupported, error.localize()));
        }
        Err(error) => incoming.respond(failed(&error)),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn delete(caller: &Caller, name: String, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let read = cx.background_executor().spawn(async { machine::list() });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let found = read
            .await
            .is_ok_and(|listings| listings.iter().any(|listing| listing.name == name));
        if !found {
            return incoming.respond(Response::error(
                ErrorCode::NotFound,
                format!("No virtual Mac is called {name}"), // i18n: data
            ));
        }
        incoming.respond(
            cx.update(|cx| offer_settings(handle, Request::Delete(Asked::Machine(name)), cx)),
        );
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn offer_settings(handle: MainWindow, request: Request, cx: &mut App) -> Response {
    match handle.update(cx, |view, _, cx| view.offer_machine_request(request, cx)) {
        Ok(true) => Response::done("Added a notice to review the request in Virtual Macs settings"),
        Ok(false) => Response::error(
            ErrorCode::WindowBusy,
            "Another virtual Mac settings request is waiting",
        ),
        Err(_) => closed(),
    }
}

/// Offers to download macOS and create a machine once the user reviews it.
#[cfg(target_os = "macos")]
fn offer(caller: &Caller, name: Option<String>, incoming: socket::Incoming, cx: &mut App) {
    let (name, incoming) = match name {
        Some(name) => match named(name, incoming) {
            Some((name, incoming)) => (Some(name), incoming),
            None => return,
        },
        None => (None, incoming),
    };
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let read = cx.background_executor().spawn(async { machine::list() });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let listings = read.await;
        let taken = name.as_ref().is_some_and(|name| {
            listings
                .as_ref()
                .is_ok_and(|listings| listings.iter().any(|listing| listing.name == *name))
        });
        if let Some(name) = name.as_ref().filter(|_| taken) {
            return incoming.respond(refused(&machine::Error::Exists(name.clone())));
        }
        incoming.respond(cx.update(|cx| offer_settings(handle, Request::Create(name), cx)));
    })
    .detach();
}

/// What a failure to make or save a machine says to the caller.
#[cfg(target_os = "macos")]
fn refused(error: &machine::Error) -> Response {
    let code = match error {
        machine::Error::ImageNotFound(_) | machine::Error::NotInstalled(_) => ErrorCode::NotFound,
        machine::Error::Busy(_) | machine::Error::Suspended(_) => ErrorCode::Busy,
        machine::Error::Exists(_) => ErrorCode::InvalidRequest,
        _ => ErrorCode::Failed,
    };
    Response::error(code, error.localize())
}

#[cfg(target_os = "macos")]
pub(super) fn new(
    caller: &Caller,
    name: Option<String>,
    from: Option<String>,
    incoming: socket::Incoming,
    cx: &mut App,
) {
    let Some(from) = from else {
        return offer(caller, name, incoming, cx);
    };
    let Some(name) = name else {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "A machine made from an image needs a name",
        ));
    };
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let Some((from, incoming)) = named(from, incoming) else {
        return;
    };
    let make = cx.background_executor().spawn(async move {
        image::create(&name, &from)?;
        Ok::<_, machine::Error>(name)
    });
    cx.spawn(async move |_: &mut gpui::AsyncApp| {
        incoming.respond(match make.await {
            Ok(name) => Response::done(format!("Made {name}")), // i18n: data
            Err(error) => refused(&error),
        });
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(super) fn image_list(incoming: socket::Incoming, cx: &mut App) {
    let read = cx.background_executor().spawn(async { image::list() });
    cx.spawn(async move |_: &mut gpui::AsyncApp| {
        incoming.respond(match read.await {
            Ok(images) => Response::Tab {
                reply: Reply::MachineImages {
                    images: images
                        .into_iter()
                        .map(|image| MachineImageInfo {
                            name: image.name.to_string(),
                            os: image.guest.os,
                            arch: image.guest.arch,
                            disk_bytes: image.disk_bytes,
                        })
                        .collect(),
                },
            },
            Err(error) => failed(&error),
        });
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(super) fn image_save(name: String, target: String, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let Some((target, incoming)) = named(target, incoming) else {
        return;
    };
    let message = format!("Saved {name} as {target}"); // i18n: data
    machine::save_image(&name, target, cx, move |saved, _| {
        incoming.respond(match saved {
            Ok(()) => Response::done(message),
            Err(error) => refused(&error),
        });
    });
}

#[cfg(target_os = "macos")]
pub(super) fn image_delete(
    caller: &Caller,
    name: String,
    incoming: socket::Incoming,
    cx: &mut App,
) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let read = cx.background_executor().spawn(async { image::list() });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let found = read
            .await
            .is_ok_and(|images| images.iter().any(|image| image.name == name));
        if !found {
            return incoming.respond(refused(&machine::Error::ImageNotFound(name)));
        }
        incoming.respond(
            cx.update(|cx| offer_settings(handle, Request::Delete(Asked::Image(name)), cx)),
        );
    })
    .detach();
}
