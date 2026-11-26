//! Tab commands target the caller's workspace. Background opens register their
//! tabs directly. Interactive commands wait for the destination to be shown.
use super::{busy, closed};
use crate::window::main_window::MainWindow;
use crate::{
    SheprdWindow,
    browser::{Location, OrchestratorRepo, Store},
    control::{
        protocol::{
            Caller, ErrorCode, Response,
            tabs::{ARRIVE_WAIT_SECONDS, Reply},
        },
        server::{Watches, socket, target, window_for},
    },
    editor::EditorTarget,
};
use gpui::{App, Context, Window};
use std::{path::Path, time::Duration};

/// The longest path a request may name.
const MAX_PATH: usize = 4096;

/// A local file a request names: absolute, bounded, and printable. Whether
/// it exists is for the window's own reading to find, off the UI thread.
fn editor_target(path: &str, line: Option<u32>) -> Result<EditorTarget, Response> {
    let valid = !path.is_empty()
        && path.len() <= MAX_PATH
        && !path.chars().any(char::is_control)
        && Path::new(path).is_absolute();
    if !valid {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            "The file must be an absolute path without control characters",
        ));
    }
    if line == Some(0) {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            "Lines are counted from 1",
        ));
    }
    Ok(EditorTarget {
        path: path.into(),
        line,
    })
}

/// The window, endpoint, and workspace a request's tab goes to.
struct Destination {
    handle: MainWindow,
    endpoint: usize,
    workspace: String,
}

fn destination(caller: &Caller, cx: &mut App) -> Result<Destination, Response> {
    let handle = window_for(caller, cx)?;
    let target = target(caller);
    let found = handle
        .read_with(cx, |view, _| {
            target
                .strictness()
                .iter()
                .find_map(|strict| view.control_target(&target, *strict))
        })
        .map_err(|_| closed())?;
    let (endpoint, workspace) = found.ok_or_else(|| {
        Response::error(
            ErrorCode::WorkspaceNotFound,
            match &caller.workspace_id {
                Some(id) => format!("No Sheprd window shows workspace {id}"),
                None => "The window shows no workspace yet".to_owned(),
            },
        )
    })?;
    Ok(Destination {
        handle,
        endpoint,
        workspace,
    })
}

/// Capture where an agent's tab belongs before asynchronous work starts.
pub(super) fn workspace(
    caller: &Caller,
    cx: &mut App,
) -> Result<(crate::browser::Scope, String), Response> {
    let dest = destination(caller, cx)?;
    let scope = dest
        .handle
        .read_with(cx, |view, _| {
            crate::browser::scope(&view.endpoints[dest.endpoint])
        })
        .map_err(|_| closed())?;
    Ok((scope, dest.workspace))
}

pub(super) fn background_tab(
    key: (crate::browser::Scope, String),
    location: Location,
    cx: &mut App,
) -> Response {
    let (scope, workspace) = key;
    match Store::update(cx, |store| store.open_location(scope, &workspace, location)) {
        Some(id) => Response::Tab {
            reply: Reply::Opened {
                id: id.into(),
                workspace,
            },
        },
        None => Response::error(ErrorCode::TabLimit, "Too many tabs are open"),
    }
}

/// Switches the window to the caller's workspace, then, once the switch
/// landed and `ready` holds, runs `act` there and answers with what it says.
fn open_there(
    caller: &Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
    ready: impl Fn(&SheprdWindow) -> bool + 'static,
    act: impl FnOnce(&mut SheprdWindow, &str, &mut Window, &mut Context<SheprdWindow>) -> Response
    + 'static,
) {
    let Destination {
        handle,
        endpoint,
        workspace,
    } = match destination(caller, cx) {
        Ok(destination) => destination,
        Err(response) => return incoming.respond(response),
    };
    let started = handle
        .update(cx, |view, _, cx| {
            if view.menu.page.is_some() {
                return Err(busy());
            }
            view.go_to_workspace(endpoint, &workspace, cx);
            Ok(())
        })
        .unwrap_or_else(|_| Err(closed()));
    if let Err(response) = started {
        return incoming.respond(response);
    }
    let expired = Response::error(
        ErrorCode::Timeout,
        format!("Workspace {workspace} was not ready in the window in time"),
    );
    let mut act = Some(act);
    watches.add(
        incoming,
        Duration::from_secs(ARRIVE_WAIT_SECONDS),
        move |cx| {
            let arrived = handle.read_with(cx, |view, _| {
                view.shows_workspace(endpoint, &workspace) && ready(view)
            });
            match arrived {
                Err(_) => return Some(closed()),
                Ok(false) => return None,
                Ok(true) => {}
            }
            let act = act.take()?;
            Some(
                handle
                    .update(cx, |view, window, cx| {
                        if view.menu.page.is_some() {
                            return busy();
                        }
                        act(view, &workspace, window, cx)
                    })
                    .unwrap_or_else(|_| closed()),
            )
        },
        expired,
        cx,
    );
}

fn open_background(
    caller: &Caller,
    incoming: socket::Incoming,
    cx: &mut App,
    open: impl FnOnce(
        &mut SheprdWindow,
        usize,
        &str,
        &mut Context<SheprdWindow>,
    ) -> Result<crate::browser::TabId, Response>,
) {
    let dest = match destination(caller, cx) {
        Ok(dest) => dest,
        Err(response) => return incoming.respond(response),
    };
    let response = dest
        .handle
        .update(cx, |view, _, cx| {
            match open(view, dest.endpoint, &dest.workspace, cx) {
                Ok(id) => Response::Tab {
                    reply: Reply::Opened {
                        id: id.into(),
                        workspace: dest.workspace,
                    },
                },
                Err(response) => response,
            }
        })
        .unwrap_or_else(|_| closed());
    incoming.respond(response);
}

pub(super) fn code(
    caller: &Caller,
    path: &str,
    line: Option<u32>,
    incoming: socket::Incoming,
    cx: &mut App,
) {
    let target = match editor_target(path, line) {
        Ok(target) => target,
        Err(response) => return incoming.respond(response),
    };
    open_background(caller, incoming, cx, |view, endpoint, workspace, cx| {
        let scope = crate::browser::scope(&view.endpoints[endpoint]);
        view.load_code_tab(&target, scope, workspace, cx)
            .ok_or_else(|| Response::error(ErrorCode::Failed, "Could not open the code tab"))
    });
}

/// Opens the file in the user's editor, in a pane split beside the
/// caller's, when the window shows it, else beside the focused pane. The
/// editor reports its failures in the window.
pub(super) fn edit(
    caller: &Caller,
    path: &str,
    line: Option<u32>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let target = match editor_target(path, line) {
        Ok(target) => target,
        Err(response) => return incoming.respond(response),
    };
    let pane = caller.pane_id.clone();
    let path = path.to_owned();
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        move |view, _, _, cx| {
            let beside = pane.as_deref().filter(|pane| {
                view.live.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.panes.iter().any(|shown| shown.pane_id == *pane)
                })
            });
            view.open_in_editor(&target, beside, cx);
            Response::done(format!(
                "Opening {path} in your editor; a problem would show in the window"
            ))
        },
    );
}

pub(super) fn devices(caller: &Caller, incoming: socket::Incoming, cx: &mut App) {
    let answer = match workspace(caller, cx) {
        Ok(key) => background_tab(key, Location::Devices, cx),
        Err(response) => response,
    };
    incoming.respond(answer);
}

/// The virtual Mac called `name`, in a tab of the caller's workspace.
/// Shows machine `name`, with its setup guide beside it when `setup`.
pub(super) fn machine(
    caller: &Caller,
    name: String,
    setup: bool,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let Ok(name) = crate::machine::Name::try_from(name) else {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "A machine name takes letters, digits, '-', and '_', and starts with a letter or digit",
        ));
    };
    let key = match workspace(caller, cx) {
        Ok(key) => key,
        Err(response) => return incoming.respond(response),
    };
    if !key.0.is_local() {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "A virtual Mac runs on this computer; open it from a workspace on this computer",
        ));
    }
    let resolved = std::rc::Rc::new(std::cell::RefCell::new(None));
    #[cfg(target_os = "macos")]
    {
        let ready = resolved.clone();
        let read = cx
            .background_executor()
            .spawn(async move { crate::machine::resolve_name(&name) });
        cx.spawn(async move |_: &mut gpui::AsyncApp| {
            *ready.borrow_mut() = Some(read.await);
        })
        .detach();
    }
    #[cfg(not(target_os = "macos"))]
    {
        *resolved.borrow_mut() = Some(Ok::<_, crate::machine::Error>(name));
    }

    watches.add(
        incoming,
        Duration::from_secs(ARRIVE_WAIT_SECONDS),
        move |cx| {
            let name = match resolved.borrow_mut().take()? {
                Ok(name) => name,
                Err(error) => return Some(Response::error(ErrorCode::Failed, error.to_string())),
            };
            #[cfg(target_os = "macos")]
            if let Err(error) = crate::machine::check_capacity(&name) {
                return Some(Response::error(ErrorCode::Busy, error.to_string()));
            }
            #[cfg(target_os = "macos")]
            if setup {
                crate::machine::guide::begin(&name, crate::machine::guide::Opening::Shown, cx);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = setup;
            let response =
                background_tab(key.clone(), Location::Machine { name: name.clone() }, cx);
            #[cfg(target_os = "macos")]
            if matches!(response, Response::Tab { .. }) {
                crate::machine::start(&name, cx);
            }
            Some(response)
        },
        Response::error(ErrorCode::Timeout, "Machine name was not resolved in time"),
        cx,
    );
}

/// The workspace's issues in the activity bar's Issues section, listing the
/// repository of `repo`, or of the workspace's focused pane.
pub(super) fn orchestrator(
    caller: &Caller,
    repo: Option<String>,
    action: Option<sheprd_control_protocol::OrchestratorRun>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    if action.as_ref().is_some_and(|action| {
        let (sheprd_control_protocol::OrchestratorRun::Restart { run }
        | sheprd_control_protocol::OrchestratorRun::Adopt { run }) = action;
        run.is_empty() || run.len() > 128 || run.chars().any(char::is_control)
    }) {
        return incoming.respond(Response::error(ErrorCode::InvalidRequest, "Invalid run ID"));
    }
    if let Some(repo) = &repo
        && OrchestratorRepo::new(repo.clone()).is_err()
    {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "The repository must be an absolute path without control characters",
        ));
    }
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        move |view, workspace, window, cx| {
            view.open_orchestrator_in(workspace.to_owned(), repo, window, cx);
            if let Some(action) = &action {
                return if view.confirm_orchestrator_run(action, cx) {
                    Response::done("Preparing the run confirmation; the user must confirm")
                } else {
                    Response::error(
                        ErrorCode::Failed,
                        "This project's run is not loaded or cannot be restarted; open its Issues section and try again",
                    )
                };
            }
            if view.activity.panel == crate::sidebar::ActivityPanel::Issues
                && view.activity_section().is_some()
            {
                Response::done(format!("Showing the issues of workspace {workspace}"))
            } else {
                Response::error(
                    ErrorCode::Unsupported,
                    "Issues show in the activity bar, which is turned off, or this host cannot list them",
                )
            }
        },
    );
}

pub(super) fn review(caller: &Caller, incoming: socket::Incoming, cx: &mut App) {
    open_background(caller, incoming, cx, |view, endpoint, workspace, cx| {
        view.background_review(
            endpoint,
            workspace,
            caller.pane_id.as_deref(),
            caller.terminal_id.as_deref(),
            cx,
        )
        .ok_or_else(|| {
            Response::error(
                ErrorCode::Failed,
                format!("Could not open a review of workspace {workspace}"),
            )
        })
    });
}

pub(super) fn px0(caller: &Caller, incoming: socket::Incoming, cx: &mut App) {
    open_background(caller, incoming, cx, |view, index, workspace, cx| {
        if !crate::browser::EMBEDDED || view.px0_root_at(index, workspace).is_none() {
            return Err(Response::error(
                ErrorCode::Unsupported,
                "px0 needs a local workspace and embedded pages (macOS or Windows)",
            ));
        }
        let scope = crate::browser::scope(&view.endpoints[index]);
        Store::update(cx, |store| {
            store.open_editor_tab(scope, workspace, crate::browser::Editor::Px0, None)
        })
        .ok_or_else(|| Response::error(ErrorCode::TabLimit, "Too many tabs are open"))
    });
}

#[cfg(test)]
mod tests;
