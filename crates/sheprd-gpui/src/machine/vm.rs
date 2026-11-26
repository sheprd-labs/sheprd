//! Live virtual machine lifecycle state on the main thread.
//!
//! A machine belongs to the app, not to a window or a tab: its tab can close
//! or move to another window while it runs. All Apple Virtualization.framework
//! types and unsafe calls stay behind [`super::backend::apple`]; this module
//! owns lifecycle transitions, view attachment policy, capacity admission, and
//! capability checks.

use super::{
    Display, Error, Name, Result, agent,
    backend::{
        StopReason,
        apple::{self, AppleVm, View},
    },
    configuration::Bundle,
    metadata::MachineCapabilities,
    quit, store,
};
use gpui::{App, AsyncApp, Task};
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
    os::fd::OwnedFd,
    path::PathBuf,
    rc::Rc,
};

/// A machine that exists, its backend handle, and its active capabilities.
#[derive(Clone)]
struct Live {
    vm: AppleVm,
    capabilities: MachineCapabilities,
    /// Whether the user asked for the display to follow the tab.
    fit: bool,
}

impl Live {
    fn auto_resize(&self) -> bool {
        self.fit && self.capabilities.dynamic_display
    }
}

enum Phase {
    Stopped,
    /// Files are being read, or the machine is booting or resuming.
    Starting(Option<Live>),
    Running(Live),
    /// Its state is being written; it stops once that is done.
    Saving(Live),
    Failed(String),
}

impl Phase {
    fn live(&self) -> Option<&Live> {
        match self {
            Self::Running(live) | Self::Saving(live) | Self::Starting(Some(live)) => Some(live),
            Self::Stopped | Self::Starting(None) | Self::Failed(_) => None,
        }
    }
}

/// What a machine is doing, for the window to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Stopped,
    Starting,
    Running,
    Saving,
    Failed(String),
}

struct Entry {
    phase: Phase,
    /// Every tab's view on it, each shown it once it runs.
    views: Vec<View>,
    /// Its files are going, so nothing may start it.
    held: bool,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            phase: Phase::Stopped,
            views: Vec::new(),
            held: false,
        }
    }
}

thread_local! {
    static MACHINES: RefCell<HashMap<Name, Entry>> = RefCell::default();
    /// For the backend's callbacks, which arrive with no `App` at hand.
    static APP: OnceCell<AsyncApp> = const { OnceCell::new() };
    /// Set as the app quits, so no machine starts that the quit does not know.
    static CLOSED: Cell<bool> = const { Cell::new(false) };
}

/// Called by the host backend when a live VM stops on its own or fails.
pub(super) fn on_backend_stopped(name: Name, reason: StopReason) {
    let phase = match reason {
        StopReason::GuestStopped => Phase::Stopped,
        StopReason::Failed(message) => Phase::Failed(message),
    };
    if let Some(app) = app() {
        app.update(|cx| set_phase(&name, phase, cx));
    }
}

/// Starts no machine from now on: the app is quitting.
pub(super) fn close() {
    CLOSED.set(true);
}

/// Whether machine `name` may start.
fn startable(name: &Name) -> bool {
    !CLOSED.get() && !with(name, |machine| machine.held)
}

pub(super) fn app() -> Option<AsyncApp> {
    APP.with(|app| app.get().cloned())
}

fn with<T>(name: &Name, read: impl FnOnce(&mut Entry) -> T) -> T {
    MACHINES.with(|machines| read(machines.borrow_mut().entry(name.clone()).or_default()))
}

/// What machine `name` is doing.
pub(crate) fn status(name: &Name) -> Status {
    MACHINES.with(
        |machines| match machines.borrow().get(name).map(|m| &m.phase) {
            None | Some(Phase::Stopped) => Status::Stopped,
            Some(Phase::Starting(_)) => Status::Starting,
            Some(Phase::Running(_)) => Status::Running,
            Some(Phase::Saving(_)) => Status::Saving,
            Some(Phase::Failed(message)) => Status::Failed(message.clone()),
        },
    )
}

/// Capabilities reported by a currently live machine, or `None` when stopped.
pub(super) fn live_capabilities(name: &Name) -> Option<MachineCapabilities> {
    MACHINES.with(|machines| {
        machines
            .borrow()
            .get(name)
            .and_then(|entry| entry.phase.live())
            .map(|live| live.capabilities)
    })
}

/// Opens a host-to-guest agent transport connection to running machine `name`.
pub(super) fn connect_agent(name: &Name, port: u32, cx: &mut App) -> Task<agent::Result<OwnedFd>> {
    let vm = MACHINES.with(
        |machines| match machines.borrow().get(name).map(|m| &m.phase) {
            Some(Phase::Running(live)) => Some(live.vm.clone()),
            _ => None,
        },
    );
    match vm {
        Some(vm) => vm.connect_vsock(name, port, cx),
        None => Task::ready(Err(agent::Error::NotRunning(name.clone()))),
    }
}

/// The machines taking this host's memory and CPU, whether a tab shows them
/// or not, by name.
pub(crate) fn running() -> Vec<Name> {
    let mut names: Vec<Name> = MACHINES.with(|machines| {
        machines
            .borrow()
            .iter()
            .filter(|(_, machine)| {
                matches!(
                    machine.phase,
                    Phase::Starting(_) | Phase::Running(_) | Phase::Saving(_)
                )
            })
            .map(|(name, _)| name.clone())
            .collect()
    });
    names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    names
}

/// Starting and saving guests still occupy a slot. Called on the app thread.
pub(crate) fn capacity() -> crate::control::protocol::tabs::MachineCapacity {
    crate::control::protocol::tabs::MachineCapacity::new(
        u32::try_from(running().len())
            .unwrap_or(u32::MAX)
            .saturating_add(u32::from(crate::machine::install::occupies_slot())),
    )
}

/// Opening an already active guest needs no new slot.
pub(crate) fn check_capacity(name: &Name) -> Result<()> {
    check_capacity_for(status(name), capacity())
}

/// [`check_capacity`] against `capacity`, which also counts other
/// programs' VMs ([`super::lend::Survey`]).
pub(crate) fn check_capacity_in(
    name: &Name,
    capacity: crate::control::protocol::tabs::MachineCapacity,
) -> Result<()> {
    check_capacity_for(status(name), capacity)
}

fn check_capacity_for(
    status: Status,
    capacity: crate::control::protocol::tabs::MachineCapacity,
) -> Result<()> {
    if matches!(status, Status::Stopped | Status::Failed(_)) && capacity.available == 0 {
        return Err(Error::Capacity {
            in_use: capacity.in_use,
            limit: capacity.limit,
        });
    }
    Ok(())
}

/// A new view on machine `name`, started if it is not running.
pub(crate) fn view(name: &Name, cx: &mut App) -> Result<View> {
    let view = apple::create_view()?;
    with(name, |machine| {
        if let Some(live) = machine.phase.live() {
            apple::attach_view(&view, &live.vm, live.auto_resize());
        }
        machine.views.push(view.clone());
    });
    start(name, cx);
    Ok(view)
}

/// Forgets `view`, whose tab closed. The machine goes on running.
pub(crate) fn release(view: &View) {
    MACHINES.with(|machines| {
        for machine in machines.borrow_mut().values_mut() {
            machine.views.retain(|held| !std::ptr::eq(&**held, &**view));
        }
    });
    apple::detach_view(view);
}

/// Records what machine `name` does now, and redraws what shows it. A machine
/// that stopped leaves its views.
fn set_phase(name: &Name, phase: Phase, cx: &mut App) {
    with(name, |machine| {
        if machine.phase.live().is_some() && phase.live().is_none() {
            for view in &machine.views {
                apple::detach_view(view);
            }
        }
        machine.phase = phase;
    });
    cx.refresh_windows();
}

fn fail(name: &Name, error: &Error, cx: &mut App) {
    tracing::warn!(%name, %error, "Machine failed");
    let message = crate::i18n::Localize::localize(error);
    set_phase(name, Phase::Failed(message), cx);
}

/// Starts machine `name` unless it is already on its way, resuming it if it
/// was saved. A failure shows through [`status`].
pub(crate) fn start(name: &Name, cx: &mut App) {
    let free = with(name, |machine| {
        matches!(machine.phase, Phase::Stopped | Phase::Failed(_))
    });
    if !free || !startable(name) {
        return;
    }
    if let Err(error) = check_capacity(name) {
        return fail(name, &error, cx);
    }
    APP.with(|app| {
        app.get_or_init(|| cx.to_async());
    });
    quit::install();
    set_phase(name, Phase::Starting(None), cx);
    let name = name.clone();
    let read = cx.background_executor().spawn({
        let name = name.clone();
        async move { Bundle::read(&name) }
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let bundle = read.await;
        cx.update(|cx| {
            // A quit or a delete came while the files were read. Nothing runs
            // yet, so the start just ends here.
            if !startable(&name) {
                return set_phase(&name, Phase::Stopped, cx);
            }
            if let Some(opening) = bundle.as_ref().ok().and_then(|bundle| bundle.guide) {
                super::guide::begin(&name, opening, cx);
            }
            let watch_linux = bundle
                .as_ref()
                .is_ok_and(|bundle| bundle.guest.os == super::metadata::GuestOs::Linux);
            if let Err(error) = bundle.and_then(|bundle| build(&name, &bundle, cx)) {
                fail(&name, &error, cx);
            } else if watch_linux {
                super::readiness::begin(&name, cx);
            }
        });
    })
    .detach();
}

/// Builds machine `name` from `bundle`, shows it in the views waiting for it,
/// and boots or resumes it.
fn build(name: &Name, bundle: &Bundle, cx: &mut App) -> Result<()> {
    let vm = AppleVm::create(name.clone(), bundle)?;
    let live = Live {
        vm,
        capabilities: bundle.capabilities(),
        fit: bundle.settings.display == Display::Fit,
    };
    with(name, |machine| {
        for view in &machine.views {
            apple::attach_view(view, &live.vm, live.auto_resize());
        }
    });
    set_phase(name, Phase::Starting(Some(live.clone())), cx);
    match &bundle.saved {
        Some(state) if live.capabilities.save_restore => restore(name, live, state.clone(), cx),
        _ => boot(name, live, cx),
    }
    Ok(())
}

fn boot(name: &Name, live: Live, cx: &mut App) {
    let vm = live.vm.clone();
    vm.start(cx.to_async(), started(name.clone(), live));
}

/// What ends a start or a resume.
fn started(name: Name, live: Live) -> impl FnOnce(Option<String>, &mut App) + 'static {
    move |error, cx| match error {
        None => set_phase(&name, Phase::Running(live), cx),
        Some(message) => fail(&name, &Error::Framework(message), cx),
    }
}

/// Removes machine `name`'s saved state off the main thread, then calls `then`.
fn discard(name: Name, cx: &mut App, then: impl FnOnce(Name, Result<()>, &mut App) + 'static) {
    let removal = cx.background_executor().spawn({
        let name = name.clone();
        async move { store::discard_state(&name) }
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let removed = removal.await;
        cx.update(|cx| then(name, removed, cx));
    })
    .detach();
}

/// Gives up on a machine whose saved state cannot be removed. Its guest must
/// not run while that state remains: the next launch would resume it over a
/// disk that moved on. Stopped here the two still match.
fn abandon(name: Name, live: &Live, error: Error, cx: &mut App) {
    halt(live, cx, move |refusal, cx| {
        if let Some(message) = refusal {
            tracing::warn!(%name, %message, "Cannot stop a machine");
        }
        fail(&name, &error, cx);
    });
}

/// Resumes from `state`. The state goes before the guest runs: resumed twice,
/// it would no longer match the disk. One that cannot be read boots afresh,
/// as after a power cut.
fn restore(name: &Name, live: Live, state: PathBuf, cx: &mut App) {
    let vm = live.vm.clone();
    let name = name.clone();
    vm.restore(&state, cx.to_async(), move |error, cx| {
        // A quit or a delete came during the restore. The guest has not run,
        // so the state still matches the disk: it stays for the next launch.
        if !startable(&name) {
            return force_stop(&name, &live, cx);
        }
        discard(name, cx, move |name, removed, cx| {
            if let Err(removal) = removed {
                return abandon(name, &live, removal, cx);
            }
            match error {
                None => {
                    let vm = live.vm.clone();
                    vm.resume(cx.to_async(), started(name, live));
                }
                Some(message) => {
                    tracing::warn!(%name, %message, "Cannot resume a machine; booting it");
                    boot(&name, live, cx);
                }
            }
        });
    });
}

/// Saves machine `name` and stops it, so it next resumes where it was. One
/// that is not running has nothing to save. Returns an unsupported-capability
/// error if the guest/backend does not support save/restore.
pub(crate) fn suspend(name: &Name, cx: &mut App) -> Result<()> {
    let live = with(name, |machine| match &machine.phase {
        Phase::Running(live) => Some(live.clone()),
        _ => None,
    });
    let Some(live) = live else {
        return Ok(());
    };
    if !live.capabilities.save_restore {
        return Err(Error::UnsupportedCapability {
            name: name.clone(),
            capability: "save/restore",
        });
    }
    let state = store::state_file(name)?;
    set_phase(name, Phase::Saving(live.clone()), cx);
    let name = name.clone();
    let vm = live.vm.clone();
    vm.pause(cx.to_async(), move |error, cx| match error {
        Some(message) => refused(&name, &live, &message, cx),
        None => save(name, live, &state, cx),
    });
    Ok(())
}

/// Writes the paused machine to `state`, then stops it.
fn save(name: Name, live: Live, state: &std::path::Path, cx: &mut App) {
    let vm = live.vm.clone();
    vm.save_state(state, cx.to_async(), move |error, cx| {
        let Some(message) = error else {
            return force_stop(&name, &live, cx);
        };
        // Half a state must not be resumed.
        discard(name, cx, move |name, removed, cx| {
            if let Err(removal) = removed {
                return abandon(name, &live, removal, cx);
            }
            let vm = live.vm.clone();
            vm.resume(cx.to_async(), move |_, cx| {
                refused(&name, &live, &message, cx);
            });
        });
    });
}

fn refused(name: &Name, live: &Live, message: &str, cx: &mut App) {
    tracing::warn!(%name, %message, "Cannot save a machine");
    set_phase(name, Phase::Running(live.clone()), cx);
}

/// Stops the machine at once, as a power cut would. `then` hears any refusal.
fn halt(live: &Live, cx: &mut App, then: impl FnOnce(Option<String>, &mut App) + 'static) {
    live.vm.stop(cx.to_async(), then);
}

/// Stops a machine whose state is saved, or whose graceful shutdown timed out.
fn force_stop(name: &Name, live: &Live, cx: &mut App) {
    let name = name.clone();
    halt(live, cx, move |error, cx| {
        if let Some(message) = error {
            tracing::warn!(%name, %message, "Cannot stop a machine");
        }
        set_phase(&name, Phase::Stopped, cx);
    });
}

/// Force-stops a running machine on app quit when save/restore is unavailable
/// and graceful shutdown did not complete in time.
pub(super) fn force_stop_running(name: &Name, cx: &mut App) {
    let live = with(name, |machine| match &machine.phase {
        Phase::Running(live) => Some(live.clone()),
        _ => None,
    });
    if let Some(live) = live {
        set_phase(name, Phase::Saving(live.clone()), cx);
        force_stop(name, &live, cx);
    }
}

/// Asks the guest to shut down, as its power button would. It may ask its
/// user first; the machine stops when it has, which the backend delegate hears.
pub(crate) fn shut_down(name: &Name) -> Result<()> {
    let live = with(name, |machine| machine.phase.live().cloned());
    let Some(live) = live else {
        return Ok(());
    };
    live.vm.request_stop()
}

/// Stops machine `name` if it runs, then deletes everything it holds. Nothing
/// starts it meanwhile. `done` hears how that went.
pub(crate) fn delete(name: &Name, cx: &mut App, done: impl FnOnce(Result<()>, &mut App) + 'static) {
    if with(name, |machine| std::mem::replace(&mut machine.held, true)) {
        return done(Err(Error::Busy(name.clone())), cx);
    }
    let name = name.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let removed = match stop_for_delete(&name, cx).await {
            Ok(()) => {
                let name = name.clone();
                cx.background_executor()
                    .spawn(async move { store::delete(&name) })
                    .await
            }
            Err(error) => Err(error),
        };
        cx.update(|cx| {
            if removed.is_ok() {
                // Tabs still naming it keep their views, so a machine made
                // later under this name shows in them.
                MACHINES.with(|machines| {
                    if let Some(entry) = machines.borrow_mut().get_mut(&name) {
                        *entry = Entry {
                            views: std::mem::take(&mut entry.views),
                            ..Entry::default()
                        };
                    }
                });
                // A machine made later under this name starts its setup over.
                super::guide::forget(&name);
                super::readiness::forget(&name);
            } else {
                with(&name, |machine| machine.held = false);
            }
            cx.refresh_windows();
            done(removed, cx);
        });
    })
    .detach();
}

/// Keeps stopped machine `name` as image `image`. Nothing starts it meanwhile.
/// `done` hears how that went.
pub(crate) fn save_image(
    name: &Name,
    image: Name,
    cx: &mut App,
    done: impl FnOnce(Result<()>, &mut App) + 'static,
) {
    if status(name) != Status::Stopped
        || with(name, |machine| std::mem::replace(&mut machine.held, true))
    {
        return done(Err(Error::Busy(name.clone())), cx);
    }
    let name = name.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let source = name.clone();
        let saved = cx
            .background_executor()
            .spawn(async move { super::image::save(&source, &image) })
            .await;
        cx.update(|cx| {
            with(&name, |machine| machine.held = false);
            done(saved, cx);
        });
    })
    .detach();
}

/// Sees machine `name` stopped, as its disk stays open until then. A start or
/// a save in progress is waited out: the framework refuses to stop a machine
/// midway through one, and their completions would write over the phase.
async fn stop_for_delete(name: &Name, cx: &mut AsyncApp) -> Result<()> {
    let refusal = Rc::new(Cell::new(None));
    loop {
        if let Some(message) = refusal.take() {
            return Err(Error::Framework(message));
        }
        let next = with(name, |machine| match &machine.phase {
            Phase::Stopped | Phase::Failed(_) => None,
            Phase::Running(live) => Some(Some(live.clone())),
            Phase::Starting(_) | Phase::Saving(_) => Some(None),
        });
        match next {
            None => return Ok(()),
            Some(Some(live)) => cx.update(|cx| {
                // Saving, so that this stop is asked for once.
                set_phase(name, Phase::Saving(live.clone()), cx);
                let name = name.clone();
                let refusal = refusal.clone();
                halt(&live.clone(), cx, move |error, cx| {
                    let phase = match error {
                        None => Phase::Stopped,
                        Some(_) => Phase::Running(live),
                    };
                    refusal.set(error);
                    set_phase(&name, phase, cx);
                });
            }),
            Some(None) => cx.background_executor().timer(quit::POLL).await,
        }
    }
}

#[cfg(test)]
mod tests;
