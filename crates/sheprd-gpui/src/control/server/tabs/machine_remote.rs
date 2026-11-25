//! `machine --host HOST ...`: a virtual Mac request run by the Sheprd on
//! another computer, which lends its machines (`machine::remote`). Taking
//! a slot there asks the user first, unless they chose to always borrow
//! from that host. The request runs on a worker; a caller that hangs up,
//! or a request that runs out of time, stops it.

use super::{App, Caller, ErrorCode, Response, TabOp, Watches, socket};
use crate::{
    automations::Machine,
    control::{
        protocol::{
            MAX_WAIT_SECONDS,
            lend::{LendAnswer, LendCall, LendRequest},
            tabs::{MAX_MACHINE_HOST, Reply},
        },
        server::{hosts::named, window_for},
    },
    i18n::Localize as _,
    machine::{
        borrow::{Answer, Answered},
        remote::{self, link},
    },
    teleport::Host,
    window::main_window::MainWindow,
};
use base64::Engine as _;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Where the request is going.
struct Target {
    host: Host,
    label: String,
    /// The endpoint, which an opened machine's tab keeps.
    endpoint: String,
}

pub(super) fn serve(
    caller: Caller,
    host: String,
    request: TabOp,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    if host.is_empty() || host.len() > MAX_MACHINE_HOST || !request.forwardable() {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "--host takes work on a virtual Mac there: open, start, claim, release, list, \
             suspend, agent, exec, screenshot, click, type, or key",
        ));
    }
    if let TabOp::MachineScreenshot { path, .. } = &request
        && !std::path::Path::new(path).is_absolute()
    {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "A screenshot's path must be absolute",
        ));
    }
    let handle = match window_for(&caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let found = handle
        .read_with(cx, |view, _| {
            let endpoint = named(view, &host)?;
            let always = view.config.machine.borrows_from(&endpoint.id)
                || view.config.machine.borrows_from(&endpoint.label);
            Ok::<_, Response>((
                endpoint.id.clone(),
                endpoint.label.clone(),
                endpoint.connection.target.clone(),
                always,
            ))
        })
        .unwrap_or_else(|_| Err(Response::error(ErrorCode::NoWindow, "The window closed")));
    let (id, label, connect, always) = match found {
        Ok(found) => found,
        Err(response) => return incoming.respond(response),
    };
    if Machine::of(&connect).is_none_or(|machine| machine.is_local()) {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            format!("{label} is this computer; leave out --host"),
        ));
    }
    let Ok(remote_host) = Host::new(&connect) else {
        return incoming.respond(Response::error(
            ErrorCode::Unsupported,
            format!("Sheprd cannot run commands on {label}"),
        ));
    };
    let target = Target {
        host: remote_host,
        label: label.clone(),
        endpoint: id.clone(),
    };
    let wait = Duration::from_secs(
        TabOp::MachineOn {
            host,
            request: Box::new(request.clone()),
        }
        .wait_seconds(),
    );
    let workspace = if matches!(request, TabOp::Machine { .. }) {
        match super::arrive::workspace(&caller, cx) {
            Ok(key) => Some(key),
            Err(response) => return incoming.respond(response),
        }
    } else {
        None
    };
    let mut job = Job {
        workspace,
        stage: Stage::Ready,
        request,
        owner: owner(&caller),
        target: Some(target),
        handle,
        running: None,
    };
    if job.request.takes_slot() && !always {
        let machine = machine_name(&job.request).to_owned();
        let key = id;
        let asked = job
            .handle
            .update(cx, |view, window, cx| {
                view.ask_to_borrow(label, key, machine, window, cx)
            })
            .ok()
            .flatten();
        match asked {
            Some(answered) => job.stage = Stage::Asking(answered),
            None => {
                return incoming.respond(Response::error(
                    ErrorCode::WindowBusy,
                    "A dialog is open in the window; close it first",
                ));
            }
        }
    }
    watches.add(
        incoming,
        wait,
        move |cx| job.poll(cx),
        Response::error(
            ErrorCode::Timeout,
            format!(
                "The other computer did not answer within {} seconds",
                wait.as_secs()
            ),
        ),
        cx,
    );
}

/// Who asks, as the lending computer shows it.
fn owner(caller: &Caller) -> String {
    match (&caller.pane_id, &caller.terminal_id) {
        (_, Some(terminal)) => format!("terminal {terminal}"),
        (Some(pane), None) => format!("pane {pane}"),
        (None, None) => "command line".to_owned(), // i18n: data
    }
}

fn machine_name(op: &TabOp) -> &str {
    match op {
        TabOp::Machine { name }
        | TabOp::MachineAgent { name }
        | TabOp::MachineClaim { name }
        | TabOp::MachineClick { name, .. }
        | TabOp::MachineExec { name, .. }
        | TabOp::MachineKey { name, .. }
        | TabOp::MachineRelease { name }
        | TabOp::MachinePeek { name, .. }
        | TabOp::MachineScreenshot { name, .. }
        | TabOp::MachineStart { name }
        | TabOp::MachineSuspend { name }
        | TabOp::MachineType { name, .. } => name,
        _ => "",
    }
}

enum Stage {
    /// Waiting for the user's answer.
    Asking(Answered),
    Ready,
    Running,
}

/// A request in flight. Dropping it, as an answered or abandoned request
/// does, stops the run if it still goes on.
struct Job {
    workspace: Option<(crate::browser::Scope, String)>,
    stage: Stage,
    request: TabOp,
    owner: String,
    target: Option<Target>,
    handle: MainWindow,
    running: Option<Running>,
}

struct Running {
    cancel: Arc<AtomicBool>,
    answer: Arc<Mutex<Option<Response>>>,
    /// The tab to open here once the machine there started.
    open: Option<(String, String)>,
}

impl Drop for Job {
    /// A request that ends while its question is open, cancelled or out of
    /// time, closes the question.
    fn drop(&mut self) {
        if let Stage::Asking(answered) = &self.stage {
            answered.cancel();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Job {
    fn poll(&mut self, cx: &mut App) -> Option<Response> {
        if let Stage::Asking(answered) = &self.stage {
            match answered.take()? {
                Answer::Once | Answer::Always => self.stage = Stage::Ready,
                Answer::WaitHere => {
                    return Some(Response::error(
                        ErrorCode::Busy,
                        "The user chose to wait for a slot on this computer; \
                         run `machine claim NAME --wait 60` without --host",
                    ));
                }
                Answer::Dismissed => {
                    return Some(Response::error(
                        ErrorCode::Busy,
                        "The user did not allow borrowing a virtual Mac there",
                    ));
                }
            }
        }
        if matches!(self.stage, Stage::Ready) {
            self.stage = Stage::Running;
            match self.start() {
                Ok(running) => self.running = Some(running),
                Err(response) => return Some(response),
            }
        }
        let running = self.running.as_ref()?;
        let response = running
            .answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        if let (Response::Done { .. }, Some((endpoint, name))) = (&response, &running.open) {
            let location = crate::browser::Location::RemoteMachine {
                endpoint: endpoint.clone(),
                name: match crate::machine::Name::try_from(name.clone()) {
                    Ok(name) => name,
                    Err(error) => {
                        return Some(Response::error(
                            ErrorCode::InvalidRequest,
                            error.to_string(),
                        ));
                    }
                },
            };
            if let Some(key) = self.workspace.take() {
                let opened = super::arrive::background_tab(key, location, cx);
                if matches!(opened, Response::Error { .. }) {
                    return Some(opened);
                }
            }
        }
        Some(response)
    }

    fn start(&mut self) -> Result<Running, Response> {
        let Some(target) = self.target.take() else {
            return Err(Response::error(
                ErrorCode::Failed,
                "The request already ran",
            ));
        };
        let (request, open) = match &self.request {
            // Opening starts it there and shows it here.
            TabOp::Machine { name } => (
                TabOp::MachineStart { name: name.clone() },
                Some((target.endpoint.clone(), name.clone())),
            ),
            op => (op.clone(), None),
        };
        let wait = Duration::from_secs(
            request
                .wait_seconds()
                .clamp(1, MAX_WAIT_SECONDS)
                .saturating_add(crate::control::protocol::tabs::MACHINE_REMOTE_SECONDS),
        );
        let running = Running {
            cancel: Arc::default(),
            answer: Arc::default(),
            open,
        };
        let cancel = running.cancel.clone();
        let answer = running.answer.clone();
        let owner = self.owner.clone();
        std::thread::Builder::new()
            .name("sheprd-machine-borrow".into())
            .spawn(move || {
                let response = run(&target, owner, request, wait, &cancel);
                *answer.lock().unwrap_or_else(PoisonError::into_inner) = Some(response);
            })
            .map_err(|error| {
                tracing::error!(category = "machine", %error, "Could not start a borrow worker");
                Response::error(ErrorCode::Failed, "Sheprd could not start a worker for it")
            })?;
        Ok(running)
    }
}

/// Runs `request` on the target, and turns its answer into this computer's.
fn run(
    target: &Target,
    owner: String,
    request: TabOp,
    wait: Duration,
    cancel: &AtomicBool,
) -> Response {
    let screenshot = match &request {
        TabOp::MachineScreenshot { name, path } => Some((name.clone(), PathBuf::from(path))),
        _ => None,
    };
    let lend = match &screenshot {
        Some((name, _)) => LendRequest::Picture { name: name.clone() },
        None => LendRequest::Tab { op: request },
    };
    let call = LendCall {
        borrower: link::BORROWER.clone(),
        owner,
        request: lend,
    };
    let answer = link::run(&target.host, &target.label, &call, wait, cancel);
    match (answer, screenshot) {
        (Ok(LendAnswer::Response { response }), _) => on_host(&target.label, response),
        (Ok(LendAnswer::Picture { png, width, height }), Some((_, path))) => {
            match write_picture(&png, width, height, &path) {
                Ok(()) => Response::Tab {
                    reply: Reply::MachineScreenshot {
                        path: path.display().to_string(),
                        width,
                        height,
                    },
                },
                Err(error) => Response::error(ErrorCode::Failed, error.localize()),
            }
        }
        (Ok(_), _) => Response::error(
            ErrorCode::Failed,
            format!("{} answered with something else", target.label), // i18n: data
        ),
        (Err(error), _) => {
            // A host that cannot lend, or be reached, exits 8 (`docs/cli.md`).
            let code = match error {
                remote::Error::Missing(_)
                | remote::Error::NotRunning(_)
                | remote::Error::Run { .. }
                | remote::Error::TooOld(_) => ErrorCode::Unsupported,
                _ => ErrorCode::Failed,
            };
            Response::error(code, error.localize())
        }
    }
}

/// The lending app's answer, its messages saying where they come from.
fn on_host(label: &str, response: Response) -> Response {
    match response {
        Response::Done { message } => Response::done(format!("On {label}: {message}")),
        Response::Error { code, message } => {
            Response::error(code, format!("On {label}: {message}"))
        }
        response => response,
    }
}

/// Checks the picture is the PNG it says, then writes it at `path` here.
fn write_picture(
    png: &str,
    width: u32,
    height: u32,
    path: &std::path::Path,
) -> Result<(), remote::Error> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png)
        .map_err(remote::Error::Base64)?;
    let size =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(|_| remote::Error::NotPng)?;
    if size != (width, height) {
        return Err(remote::Error::NotPng);
    }
    std::fs::write(path, bytes).map_err(|source| remote::Error::Write {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests;
