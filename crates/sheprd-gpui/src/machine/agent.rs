//! Sheprd Agent: how agents work inside a virtual Mac. A small app in the
//! guest (`macos/sheprd-machine-agent`) runs commands, takes screenshots,
//! and posts clicks and keys in the logged-in session, and answers here
//! over the machine's vsock device: no network, and nothing outside the
//! machine can reach it. The user installs it once from the disk every
//! machine boots with ([`super::configuration`]), then keeps it in an image.
//!
//! The connection is made on the main thread, the machine's queue; the
//! request and its reply cross it on a background thread.
//!
//! `objc2-virtualization` marks every call `unsafe`, as it does not check
//! the framework's threading rules. Each call here runs on the main thread.
#![allow(unsafe_code)]

mod error;
mod wire;

pub use error::Error;
pub(crate) use wire::{Exec, Hello, Refusal, Request};

use super::{Name, vm};
use base64::Engine as _;
use block2::RcBlock;
use gpui::{App, AsyncApp, Task};
use objc2_foundation::NSError;
use objc2_virtualization::{VZVirtioSocketConnection, VZVirtioSocketDevice};
use std::{
    cell::Cell,
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// How long the guest may take to accept a connection.
const CONNECT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);
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
    let call = call(name, request, Duration::ZERO, cx);
    cx.background_executor().spawn(async move {
        let wire::Done {} = wire::parse(&call.await?)?;
        Ok(())
    })
}

/// Sends `request` to the agent in machine `name`, and hands back the reply
/// it read, given `wait` beyond the usual time.
fn call(name: &Name, request: Request, wait: Duration, cx: &mut App) -> Task<Result<Vec<u8>>> {
    let connecting = connect(name, cx);
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

/// Opens a connection to the agent in machine `name`, which must be running.
fn connect(name: &Name, cx: &mut App) -> Task<Result<OwnedFd>> {
    let device = vm::running_machine(name).and_then(|machine| {
        // SAFETY: on the main thread, the machine's queue.
        let devices = unsafe { machine.socketDevices() };
        devices
            .firstObject()
            .and_then(|device| device.downcast::<VZVirtioSocketDevice>().ok())
    });
    let Some(device) = device else {
        return Task::ready(Err(Error::NotRunning(name.clone())));
    };
    let answer: Rc<Cell<Option<Result<OwnedFd>>>> = Rc::default();
    let block = {
        let answer = answer.clone();
        let name = name.clone();
        RcBlock::new(
            move |connection: *mut VZVirtioSocketConnection, error: *mut NSError| {
                answer.set(Some(opened(&name, connection, error)));
            },
        )
    };
    // SAFETY: on the machine's queue; the framework copies the block, and
    // calls it there.
    unsafe { device.connectToPort_completionHandler(wire::PORT, &block) };
    let name = name.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let start = Instant::now();
        loop {
            if let Some(answer) = answer.take() {
                return answer;
            }
            if start.elapsed() > CONNECT {
                return Err(Error::Unreachable {
                    name,
                    reason: "no answer".to_owned(), // i18n: data
                });
            }
            cx.background_executor().timer(POLL).await;
        }
    })
}

/// The connection's own descriptor, which outlives it: the connection is
/// closed here, and the copy is the caller's.
fn opened(
    name: &Name,
    connection: *mut VZVirtioSocketConnection,
    error: *mut NSError,
) -> Result<OwnedFd> {
    // SAFETY: Virtualization passes null or objects that outlive this call.
    let (connection, error) = unsafe { (connection.as_ref(), error.as_ref()) };
    let unreachable = |reason: String| Error::Unreachable {
        name: name.clone(),
        reason,
    };
    let Some(connection) = connection else {
        return Err(unreachable(error.map_or_else(String::new, vm::description)));
    };
    // SAFETY: the descriptor stays open until the connection closes below,
    // after it was copied.
    let copied = unsafe {
        let fd = std::os::fd::BorrowedFd::borrow_raw(connection.fileDescriptor());
        let copied = fd.try_clone_to_owned();
        connection.close();
        copied
    };
    copied.map_err(Error::Io)
}

#[cfg(test)]
mod tests;
