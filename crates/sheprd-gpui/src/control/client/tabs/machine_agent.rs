//! `machine agent`, `exec`, and `screenshot`: what comes back from inside a
//! virtual Mac. `exec` prints as `cloud exec` does.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::ExecCommand,
    control::{
        client::hosts::exec,
        protocol::{
            Request, Response,
            tabs::{MachineAgentInfo, Reply, TabOp},
        },
    },
};
use std::path::Path;

pub(super) fn agent(name: String, context: &Context) -> crate::Result<Output> {
    let op = TabOp::MachineAgent { name: name.clone() };
    match context.call(Request::Tab, op)? {
        Response::Tab {
            reply: Reply::MachineAgent(info),
        } => Output::data(describe(&name, &info), &info),
        response => Err(unexpected(response)),
    }
}

/// What the agent reports, for a person.
pub(super) fn describe(name: &str, info: &MachineAgentInfo) -> String {
    let allowed = |allowed: bool| {
        if allowed {
            "allowed"
        } else {
            "not allowed yet"
        }
    };
    format!(
        "Sheprd Agent {} answers in {name}\n\
         Screen: {}x{} pixels ({}x), the unit `machine click` takes\n\
         Accessibility (click, type, key): {}\n\
         Screen Recording (screenshot): {}",
        info.version,
        info.width,
        info.height,
        info.scale,
        allowed(info.accessibility),
        allowed(info.screen_recording),
    ) // i18n: data
}

pub(super) fn exec(command: ExecCommand, context: &Context) -> crate::Result<Output> {
    let op = TabOp::MachineExec {
        name: command.device,
        command: command.command,
        cwd: command.cwd,
        wait_seconds: command.timeout,
    };
    match context.call(Request::Tab, op)? {
        Response::Tab {
            reply: Reply::MachineExec(info),
        } => exec::output(&info),
        response => Err(unexpected(response)),
    }
}

/// The app writes the picture, so the path is made absolute here, where the
/// caller's working directory is known.
pub(super) fn screenshot(name: String, path: &Path, context: &Context) -> crate::Result<Output> {
    let path = std::path::absolute(path)?;
    let path = path
        .to_str()
        .ok_or(crate::Error::InvalidLocalPage)?
        .to_owned();
    match context.call(Request::Tab, TabOp::MachineScreenshot { name, path })? {
        Response::Tab {
            reply: reply @ Reply::MachineScreenshot { .. },
        } => {
            let text = match &reply {
                Reply::MachineScreenshot {
                    path,
                    width,
                    height,
                } => format!("Saved {path} ({width}x{height})"), // i18n: data
                _ => String::new(),
            };
            Output::data(text, &reply)
        }
        response => Err(unexpected(response)),
    }
}

#[cfg(test)]
mod tests;
