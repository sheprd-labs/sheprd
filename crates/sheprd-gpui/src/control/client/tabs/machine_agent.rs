//! `machine agent`, `exec`, and `screenshot`: what comes back from inside a
//! virtual Mac. `exec` prints as `cloud exec` does.
use super::{Machines, Output, unexpected};
use crate::{
    cli::control::ExecCommand,
    control::{
        client::hosts::exec,
        protocol::{
            Response,
            tabs::{MachineAgentInfo, Reply, TabOp},
        },
    },
};
use std::path::Path;

pub(super) fn agent(name: String, on: &Machines) -> crate::Result<Output> {
    let op = TabOp::MachineAgent { name: name.clone() };
    match on.call(op)? {
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

pub(super) fn exec(command: ExecCommand, on: &Machines) -> crate::Result<Output> {
    let op = TabOp::MachineExec {
        name: command.device,
        command: command.command,
        cwd: command.cwd,
        wait_seconds: command.timeout,
    };
    match on.call(op)? {
        Response::Tab {
            reply: Reply::MachineExec(info),
        } => exec::output(&info),
        response => Err(unexpected(response)),
    }
}

/// The app writes the picture, so the path is made absolute here, where the
/// caller's working directory is known.
pub(super) fn screenshot(name: String, path: &Path, on: &Machines) -> crate::Result<Output> {
    let path = std::path::absolute(path)?;
    let path = path
        .to_str()
        .ok_or(crate::Error::InvalidLocalPage)?
        .to_owned();
    match on.call(TabOp::MachineScreenshot { name, path })? {
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

/// Wait in the CLI process, never on the app thread. Progress stays on
/// stderr so --json retains one machine-readable result on stdout.
pub(super) fn claim(name: String, wait: u64, on: &Machines) -> crate::Result<Output> {
    use crate::control::protocol::ErrorCode;
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(wait);
    loop {
        match on.act(TabOp::MachineClaim { name: name.clone() }) {
            Err(crate::Error::Control(crate::control::Error::Rejected {
                code: ErrorCode::Busy,
                ref message,
            })) if Instant::now() < deadline => {
                eprintln!("Waiting for an available macOS VM: {message}");
                std::thread::sleep(
                    Duration::from_secs(5).min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests;
