//! `machine agent`, `exec`, `screenshot`, `click`, `type`, and `key`: work
//! inside a virtual Mac through Sheprd Agent there. None asks first: like
//! `cloud exec`, it is the agent's own work on a machine the user made for
//! it, and it reaches nothing outside that machine.

use super::*;
#[cfg(target_os = "macos")]
use crate::{
    control::protocol::{
        MAX_WAIT_SECONDS,
        cloud::ExecInfo,
        tabs::{MAX_MACHINE_TYPE_CHARS, MachineAgentInfo},
    },
    i18n::Localize as _,
    machine::{
        Name,
        agent::{self, Request},
    },
};
#[cfg(target_os = "macos")]
use std::{path::PathBuf, time::Duration};

#[cfg(not(target_os = "macos"))]
pub(super) fn serve(_: TabOp, incoming: socket::Incoming, _: &mut App) {
    incoming.respond(Response::error(
        ErrorCode::Unsupported,
        "Virtual Macs need an Apple silicon Mac",
    ));
}

#[cfg(target_os = "macos")]
pub(super) fn serve(op: TabOp, incoming: socket::Incoming, cx: &mut App) {
    let (name, work) = match op {
        TabOp::MachineAgent { name } => (name, Work::Hello),
        TabOp::MachineExec {
            name,
            command,
            cwd,
            wait_seconds,
        } => (name, Work::Exec(command, cwd, wait_seconds)),
        TabOp::MachineScreenshot { name, path } => (name, Work::Screenshot(path.into())),
        TabOp::MachineClick {
            name,
            x,
            y,
            button,
            count,
        } => (
            name,
            Work::Post(Request::Click {
                x,
                y,
                button,
                count,
            }),
        ),
        TabOp::MachineType { name, text } => (name, Work::Post(Request::Type { text })),
        TabOp::MachineKey {
            name,
            key,
            modifiers,
        } => (name, Work::Post(Request::Key { key, modifiers })),
        _ => {
            return incoming.respond(Response::error(
                ErrorCode::InvalidRequest,
                "Not a machine agent request",
            ));
        }
    };
    let name = match Name::try_from(name) {
        Ok(name) => name,
        Err(error) => {
            return incoming.respond(Response::error(
                ErrorCode::InvalidRequest,
                error.to_string(),
            ));
        }
    };
    if let Err(message) = work.check() {
        return incoming.respond(Response::error(ErrorCode::InvalidRequest, message));
    }
    work.run(name, incoming, cx);
}

/// A request, read and checked.
#[cfg(target_os = "macos")]
enum Work {
    Hello,
    Exec(Vec<String>, Option<String>, u64),
    Screenshot(PathBuf),
    Post(Request),
}

#[cfg(target_os = "macos")]
impl Work {
    /// What is wrong with the request, before the machine is reached.
    fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Exec(command, ..) if command.first().is_none_or(String::is_empty) => {
                Err("exec needs a command")
            }
            Self::Exec(_, _, wait) if *wait == 0 || *wait > MAX_WAIT_SECONDS => {
                Err("--timeout is 1 to 600 seconds")
            }
            Self::Screenshot(path) if !path.is_absolute() => {
                Err("A screenshot's path must be absolute")
            }
            Self::Post(Request::Type { text }) if text.chars().count() > MAX_MACHINE_TYPE_CHARS => {
                Err("type takes at most 4096 characters at once")
            }
            Self::Post(Request::Click { count, .. }) if !(1..=3).contains(count) => {
                Err("A click is single, double, or triple")
            }
            _ => Ok(()),
        }
    }

    fn run(self, name: Name, incoming: socket::Incoming, cx: &mut App) {
        let device = name.to_string();
        let reply: gpui::Task<agent::Result<Response>> = match self {
            Self::Hello => {
                let task = agent::hello(&name, cx);
                cx.background_executor().spawn(async move {
                    let hello = task.await?;
                    Ok(tab(Reply::MachineAgent(MachineAgentInfo {
                        version: hello.version,
                        width: hello.width,
                        height: hello.height,
                        scale: scale(hello.scale),
                        accessibility: hello.accessibility,
                        screen_recording: hello.screen_recording,
                    })))
                })
            }
            Self::Exec(command, cwd, wait) => {
                let task = agent::exec(&name, command, cwd, Duration::from_secs(wait), cx);
                cx.background_executor().spawn(async move {
                    let exec = task.await?;
                    Ok(tab(Reply::MachineExec(Box::new(ExecInfo {
                        device,
                        status: exec.status.filter(|_| !exec.timed_out),
                        stdout: exec.stdout,
                        stderr: exec.stderr,
                        truncated: exec.truncated,
                    }))))
                })
            }
            Self::Screenshot(path) => {
                let task = agent::screenshot(&name, path.clone(), cx);
                cx.background_executor().spawn(async move {
                    let (width, height) = task.await?;
                    Ok(tab(Reply::MachineScreenshot {
                        path: path.display().to_string(),
                        width,
                        height,
                    }))
                })
            }
            Self::Post(request) => {
                let task = agent::post(&name, request, cx);
                cx.background_executor().spawn(async move {
                    task.await?;
                    Ok(Response::done(format!("Done in {device}"))) // i18n: data
                })
            }
        };
        cx.background_executor()
            .spawn(async move {
                incoming.respond(reply.await.unwrap_or_else(|error| refused(&error)));
            })
            .detach();
    }
}

#[cfg(target_os = "macos")]
fn tab(reply: Reply) -> Response {
    Response::Tab { reply }
}

/// Pixels per point, as a whole number: 1 or 2 on the displays macOS offers.
#[cfg(target_os = "macos")]
fn scale(scale: f64) -> u8 {
    if scale.is_finite() && scale >= 1.0 {
        scale.round().min(f64::from(u8::MAX)) as u8
    } else {
        1
    }
}

/// What a failure inside a machine says to the caller.
#[cfg(target_os = "macos")]
fn refused(error: &agent::Error) -> Response {
    let code = match error {
        agent::Error::NotRunning(_) => ErrorCode::Busy,
        agent::Error::Refused {
            refusal: agent::Refusal::Invalid,
            ..
        } => ErrorCode::InvalidRequest,
        _ => ErrorCode::Failed,
    };
    Response::error(code, error.localize())
}
