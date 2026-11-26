//! Desktop readiness tracking for Linux virtual machines.
//!
//! Linux guests do not use the macOS setup guide. Instead, once a Linux VM
//! starts running, Sheprd probes `agent::hello` until the guest reports
//! `capabilities.desktop_ready`, then marks the machine ready on disk so saved
//! images inherit readiness and UI/CLI surfaces can report provisioning vs.
//! desktop readiness clearly.

use super::{Name, Status, agent, status, store};
use gpui::{App, AsyncApp};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    time::Duration,
};

/// How often readiness polls the guest agent while waiting for desktop ready.
const POLL: Duration = Duration::from_secs(2);

/// Current boot/provisioning/desktop readiness of a Linux machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Readiness {
    NotRunning,
    Booting,
    Configuring,
    Ready,
    Failed(String),
}

struct Watch {
    generation: u64,
    state: Readiness,
    marked_on_disk: bool,
}

thread_local! {
    static WATCHES: RefCell<HashMap<Name, Watch>> = RefCell::new(HashMap::new());
    static NEXT_GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Current live readiness for `name`.
#[allow(dead_code)] // Read by Settings UI and CLI status in Phase 5.
pub(crate) fn state(name: &Name) -> Readiness {
    let vm_status = status(name);
    let recorded = WATCHES.with(|watches| watches.borrow().get(name).map(|w| w.state.clone()));
    match (vm_status, recorded) {
        (Status::Failed(message), _) => Readiness::Failed(message),
        (Status::Stopped | Status::Saving, _) => Readiness::NotRunning,
        (Status::Starting, _) => Readiness::Booting,
        (Status::Running, Some(state)) => state,
        (Status::Running, None) => Readiness::Booting,
    }
}

/// Starts watching desktop readiness for Linux machine `name`.
pub(super) fn begin(name: &Name, cx: &mut App) {
    let generation = WATCHES.with(|watches| {
        let generation = NEXT_GENERATION.get() + 1;
        NEXT_GENERATION.set(generation);
        watches.borrow_mut().insert(
            name.clone(),
            Watch {
                generation,
                state: Readiness::Booting,
                marked_on_disk: false,
            },
        );
        generation
    });
    watch(name.clone(), generation, cx);
}

/// Drops readiness tracking for `name` (e.g., when deleted).
pub(super) fn forget(name: &Name) {
    WATCHES.with(|watches| watches.borrow_mut().remove(name));
}

/// Pure transition function from VM status + agent hello reply to [`Readiness`].
fn evaluate(vm_status: &Status, previous: &Readiness, hello: Option<&agent::Hello>) -> Readiness {
    match vm_status {
        Status::Stopped | Status::Saving => Readiness::NotRunning,
        Status::Failed(message) => Readiness::Failed(message.clone()),
        Status::Starting => Readiness::Booting,
        Status::Running => match hello {
            Some(hello) if hello.effective_capabilities().desktop_ready => Readiness::Ready,
            Some(_) => Readiness::Configuring,
            None if *previous == Readiness::Ready => Readiness::Ready,
            None => Readiness::Booting,
        },
    }
}

fn watch(name: Name, generation: u64, cx: &mut App) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            let step = cx.update(|cx| {
                let current_status = status(&name);
                let active = WATCHES.with(|watches| {
                    let mut watches = watches.borrow_mut();
                    let watch = watches.get_mut(&name)?;
                    if watch.generation != generation {
                        return None;
                    }
                    match &current_status {
                        Status::Stopped => {
                            watch.state = Readiness::NotRunning;
                            Some(LoopAction::Stop)
                        }
                        Status::Failed(message) => {
                            watch.state = Readiness::Failed(message.clone());
                            Some(LoopAction::Stop)
                        }
                        Status::Starting | Status::Saving => {
                            watch.state = evaluate(&current_status, &watch.state, None);
                            Some(LoopAction::Wait)
                        }
                        Status::Running if watch.state == Readiness::Ready => {
                            Some(LoopAction::Stop)
                        }
                        Status::Running => Some(LoopAction::Probe),
                    }
                });
                cx.refresh_windows();
                active
            });

            match step {
                None | Some(LoopAction::Stop) => break,
                Some(LoopAction::Wait) => {}
                Some(LoopAction::Probe) => {
                    let probe = cx.update(|cx| agent::hello(&name, cx));
                    let answer = probe.await.ok();
                    let mark_disk = cx.update(|cx| {
                        let current_status = status(&name);
                        let mut should_mark = false;
                        WATCHES.with(|watches| {
                            if let Some(watch) = watches.borrow_mut().get_mut(&name)
                                && watch.generation == generation
                            {
                                watch.state =
                                    evaluate(&current_status, &watch.state, answer.as_ref());
                                if watch.state == Readiness::Ready && !watch.marked_on_disk {
                                    watch.marked_on_disk = true;
                                    should_mark = true;
                                }
                            }
                        });
                        cx.refresh_windows();
                        should_mark
                    });
                    if mark_disk {
                        let name = name.clone();
                        cx.background_executor()
                            .spawn(async move {
                                if let Err(error) = store::mark_ready(&name) {
                                    tracing::warn!(
                                        %name,
                                        %error,
                                        "Cannot mark Linux machine desktop ready"
                                    );
                                }
                            })
                            .detach();
                        break;
                    }
                }
            }
            cx.background_executor().timer(POLL).await;
        }
    })
    .detach();
}

enum LoopAction {
    Probe,
    Wait,
    Stop,
}

#[cfg(test)]
mod tests;
