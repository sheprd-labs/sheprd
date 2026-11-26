//! Saving or gracefully shutting down machines as the app quits.
//!
//! GPUI gives its quit handlers 200 ms, and writing a machine's memory or
//! waiting for ACPI shutdown takes longer. Every way out passes through
//! AppKit's `applicationShouldTerminate:`, which GPUI's delegate does not
//! answer and offers no hook for, so this adds the method to the delegate's
//! class.
//!
//! How to wait depends on who asked, because of where the question runs:
//!
//! - GPUI's own quit (the menu, the keys, `app quit`, closing the last window,
//!   an update's restart) calls `terminate:` from a block on the main queue.
//!   "Terminate later" would spin an event loop inside that block, and the
//!   serial queue would never reach the machine's completion handlers, so the
//!   app would hang. So the quit is cancelled, and tried again once the
//!   machines are saved or stopped.
//! - Anyone else (the Dock, logging out, shutting down) asks through an Apple
//!   Event, outside any block. Cancelling would abort a logout, so these
//!   "terminate later", and the event loop delivers the handlers.
//!
//! The method and the runtime call that adds it need `unsafe`, as neither
//! `objc2` nor `objc2-app-kit` has a safe form for adding to a class they did
//! not define.
#![allow(unsafe_code)]

use super::{
    metadata::MachineCapabilities,
    vm::{self, Status},
};
use gpui::AsyncApp;
use objc2::{
    MainThreadMarker, ffi,
    runtime::{AnyClass, AnyObject, Imp, Sel},
    sel,
};
use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
use objc2_foundation::NSAppleEventManager;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

/// How often the quit looks at the saves/shutdowns, and how long it waits for
/// them. A save stuck past this loses that machine's state, not the user's quit.
pub(super) const POLL: Duration = Duration::from_millis(100);
const LIMIT: Duration = Duration::from_secs(120);
/// Maximum time a guest without save/restore gets to complete graceful ACPI
/// shutdown before it is force-stopped so app quit never hangs indefinitely.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// Set while saves/shutdowns hold the quit back, so a second quit does not
    /// start a second wait.
    static WAITING: Cell<bool> = const { Cell::new(false) };
    /// Set once the quit wait is over, so the quit goes through whatever became
    /// of them: a machine whose save or shutdown failed must not hold the app
    /// forever.
    static LEAVING: Cell<bool> = const { Cell::new(false) };
}

/// Answers `applicationShouldTerminate:` from now on.
pub(super) fn install() {
    if INSTALLED.replace(true) {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(mtm);
    let Some(delegate) = application.delegate() else {
        return;
    };
    let object: &AnyObject = (*delegate).as_ref();
    let class: &AnyClass = object.class();
    let answer: extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> NSApplicationTerminateReply =
        should_terminate;
    // SAFETY: `answer` has the method's signature, which `types` spells: an
    // NSUInteger from self, the selector, and the sender. A class that has
    // the method already keeps its own.
    unsafe {
        let imp: Imp = std::mem::transmute(answer);
        let types = c"Q@:@";
        ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            sel!(applicationShouldTerminate:),
            imp,
            types.as_ptr(),
        );
    }
    // AppKit may note what a delegate answers as it is set.
    application.setDelegate(Some(&delegate));
}

extern "C-unwind" fn should_terminate(
    _: &AnyObject,
    _: Sel,
    _: &AnyObject,
) -> NSApplicationTerminateReply {
    let running = vm::running();
    let app = vm::app().filter(|_| !running.is_empty() && !LEAVING.get());
    let Some(app) = app else {
        return NSApplicationTerminateReply::TerminateNow;
    };
    let event = NSAppleEventManager::sharedAppleEventManager().currentAppleEvent();
    let way = if event.is_some() {
        Way::Reply
    } else {
        Way::Again
    };
    if !WAITING.replace(true) {
        vm::close();
        wait(app, running, way);
    }
    match way {
        Way::Reply => NSApplicationTerminateReply::TerminateLater,
        Way::Again => NSApplicationTerminateReply::TerminateCancel,
    }
}

/// How a quit held back for saves or shutdowns goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    /// AppKit is waiting for the answer.
    Reply,
    /// The quit was cancelled, so the app quits again.
    Again,
}

/// What the quit does about a machine it holds for, given its capabilities,
/// whether it was already asked to save/stop, and how long a graceful shutdown
/// has been running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Done,
    Suspend,
    ShutDown,
    ForceStop,
    Wait,
}

fn step(
    status: &Status,
    capabilities: MachineCapabilities,
    asked: bool,
    shutdown_elapsed: Option<Duration>,
    forced: bool,
) -> Step {
    match status {
        // A machine on its way up cannot save yet, and one resuming has
        // already given up its old state, so it is waited for.
        Status::Starting | Status::Saving => Step::Wait,
        Status::Running if capabilities.save_restore => {
            if !asked {
                Step::Suspend
            } else {
                Step::Done
            }
        }
        Status::Running if capabilities.graceful_shutdown => match shutdown_elapsed {
            None => Step::ShutDown,
            Some(elapsed) if elapsed < SHUTDOWN_GRACE => Step::Wait,
            Some(_) if !forced => Step::ForceStop,
            Some(_) => Step::Done,
        },
        Status::Running => {
            if !forced {
                Step::ForceStop
            } else {
                Step::Done
            }
        }
        Status::Stopped | Status::Failed(_) => Step::Done,
    }
}

/// Saves or gracefully shuts down each machine of `names`, which [`vm::close`]
/// keeps from growing, and lets the app go once none is left. One whose save
/// or shutdown failed or outlasted its budget never holds the user's quit.
fn wait(app: AsyncApp, names: Vec<super::Name>, way: Way) {
    let waiter = app.clone();
    app.spawn(async move |_: &mut AsyncApp| {
        let start = Instant::now();
        let mut asked = HashSet::new();
        let mut shutting_down: HashMap<super::Name, Instant> = HashMap::new();
        let mut forced = HashSet::new();
        while start.elapsed() < LIMIT {
            let mut held = false;
            for name in &names {
                let capabilities = vm::live_capabilities(name).unwrap_or_default();
                let shutdown_elapsed = shutting_down.get(name).map(Instant::elapsed);
                match step(
                    &vm::status(name),
                    capabilities,
                    asked.contains(name),
                    shutdown_elapsed,
                    forced.contains(name),
                ) {
                    Step::Done => {}
                    Step::Suspend => {
                        asked.insert(name.clone());
                        waiter.update(|cx| {
                            if let Err(error) = vm::suspend(name, cx) {
                                tracing::warn!(%name, %error, "Cannot suspend machine on quit");
                            }
                        });
                        held = true;
                    }
                    Step::ShutDown => {
                        shutting_down.insert(name.clone(), Instant::now());
                        if let Err(error) = vm::shut_down(name) {
                            tracing::warn!(%name, %error, "Graceful shutdown failed on quit; force stopping");
                            forced.insert(name.clone());
                            waiter.update(|cx| vm::force_stop_running(name, cx));
                        }
                        held = true;
                    }
                    Step::ForceStop => {
                        forced.insert(name.clone());
                        waiter.update(|cx| vm::force_stop_running(name, cx));
                        held = true;
                    }
                    Step::Wait => held = true,
                }
            }
            if !held {
                break;
            }
            waiter.background_executor().timer(POLL).await;
        }
        WAITING.set(false);
        LEAVING.set(true);
        match way {
            Way::Again => waiter.update(|cx| cx.quit()),
            Way::Reply => {
                if let Some(mtm) = MainThreadMarker::new() {
                    NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(true);
                }
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests;
