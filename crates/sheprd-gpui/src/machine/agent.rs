//! Sheprd Agent: how agents work inside a virtual machine. A guest agent runs
//! in the logged-in graphical session to run commands, capture screenshots,
//! and post input, answering the host over the machine's vsock transport: no
//! network, and nothing outside the virtual machine can reach it.
//!
//! Transport opening stays behind [`super::vm::connect_agent`] (implemented by
//! the host backend); framing and request/reply validation run on background
//! threads in pure safe Rust.

mod error;
mod wire;

pub use error::Error;
pub(crate) use wire::{Exec, Hello, Refusal, Request};

use super::{Name, vm};
use base64::Engine as _;
use gpui::{App, AsyncApp, Task};
use sheprd_control_protocol::protocol::tabs::{MACHINE_TYPE_GAP_MS, MACHINE_TYPE_SECONDS};
use std::{os::unix::net::UnixStream, path::PathBuf, time::Duration};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// How long the guest may take to accept a transport connection.
pub(crate) const CONNECT: Duration = Duration::from_secs(5);
/// How long a reply may take, beyond what the request waits for itself.
const SLACK: Duration = Duration::from_secs(20);
/// The most `exec` hands back of each stream, as `cloud exec` does.
const STDOUT_LIMIT: usize = 64 * 1024;
const STDERR_LIMIT: usize = 32 * 1024;

/// What the agent in machine `name` reports.
pub(crate) fn hello(name: &Name, cx: &mut App) -> Task<Result<Hello>> {
    let call = call(name, Request::Hello, Duration::ZERO, cx);
    cx.background_executor()
        .spawn(async move { wire::parse(&call.await?) })
}

/// Runs `argv` in machine `name` for at most `timeout`. Each stream comes
/// back cut to its end.
pub(crate) fn exec(
    name: &Name,
    argv: Vec<String>,
    cwd: Option<String>,
    timeout: Duration,
    cx: &mut App,
) -> Task<Result<Exec>> {
    let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    let request = Request::Exec {
        argv,
        cwd,
        timeout_ms,
    };
    let call = call(name, request, timeout, cx);
    cx.background_executor().spawn(async move {
        let mut exec: Exec = wire::parse(&call.await?)?;
        let (stdout, cut_out) = wire::tail(exec.stdout, STDOUT_LIMIT);
        let (stderr, cut_err) = wire::tail(exec.stderr, STDERR_LIMIT);
        exec.stdout = stdout;
        exec.stderr = stderr;
        exec.truncated |= cut_out || cut_err;
        Ok(exec)
    })
}

/// Writes a PNG of machine `name`'s screen to `path`, and hands back its
/// size in pixels.
pub(crate) fn screenshot(name: &Name, path: PathBuf, cx: &mut App) -> Task<Result<(u32, u32)>> {
    let call = call(name, Request::Screenshot, Duration::ZERO, cx);
    cx.background_executor().spawn(async move {
        let picture: wire::Picture = wire::parse(&call.await?)?;
        let png = base64::engine::general_purpose::STANDARD
            .decode(picture.png)
            .map_err(Error::Picture)?;
        if wire::png_size(&png) != Some((picture.width, picture.height)) {
            return Err(Error::NotPng);
        }
        std::fs::write(&path, png).map_err(|source| Error::Write { path, source })?;
        Ok((picture.width, picture.height))
    })
}

/// Posts a click, text, or a key: whatever has nothing to hand back.
pub(crate) fn post(name: &Name, request: Request, cx: &mut App) -> Task<Result<()>> {
    let wait = posting(&request);
    let call = call(name, request, wait, cx);
    cx.background_executor().spawn(async move {
        let wire::Done {} = wire::parse(&call.await?)?;
        Ok(())
    })
}

/// How long the agent may take to post `request`: its own typing, after
/// any typing already under way, as it posts one request's input at a time.
fn posting(request: &Request) -> Duration {
    let characters = match request {
        Request::Type { text } => text.chars().count(),
        _ => 0,
    };
    let gap = Duration::from_millis(MACHINE_TYPE_GAP_MS);
    gap.saturating_mul(u32::try_from(characters).unwrap_or(u32::MAX))
        + Duration::from_secs(MACHINE_TYPE_SECONDS)
}

/// Sends `request` to the agent in machine `name`, and hands back the reply
/// it read, given `wait` beyond the usual time.
fn call(name: &Name, request: Request, wait: Duration, cx: &mut App) -> Task<Result<Vec<u8>>> {
    let connecting = vm::connect_agent(name, wire::PORT, cx);
    let background = cx.background_executor().clone();
    cx.spawn(async move |_: &mut AsyncApp| {
        let stream = UnixStream::from(connecting.await?);
        background
            .spawn(async move {
                stream
                    .set_read_timeout(Some(wait + SLACK))
                    .map_err(Error::Io)?;
                stream.set_write_timeout(Some(SLACK)).map_err(Error::Io)?;
                wire::exchange(&stream, &request)
            })
            .await
    })
}

#[cfg(test)]
mod tests;
