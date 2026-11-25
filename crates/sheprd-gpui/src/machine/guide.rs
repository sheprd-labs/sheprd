//! The setup guide a machine's tab shows beside it: the steps that make it
//! ready for agents (sign in, install Sheprd Agent, allow its two
//! permissions, keep the machine as an image). Sheprd checks each step
//! through the agent and moves on by itself; signing in is the one it cannot
//! see until the agent runs, so the user says when it is done.
//!
//! A machine just installed starts with its guide open (the `GUIDE` file in
//! its folder), and Settings opens it for any machine. One made from an image
//! whose agent was seen set up has none; one made from an image saved before
//! Sheprd kept that note opens it quietly ([`Opening::Quiet`]), as the image
//! may well be set up. Once an agent answers with both permissions, its
//! machine is marked ready, and so is every image saved from it. The guide
//! lives here, in memory, so every window showing the machine agrees.

use super::{Name, Status, agent, image, status, store};
use crate::i18n::Localize as _;
use gpui::{App, AsyncApp};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    time::{Duration, Instant},
};

/// How often a waiting step asks the agent again.
const EVERY: Duration = Duration::from_secs(2);
/// How long a quiet guide waits, while its machine runs, for the agent to
/// say whether it is needed before it shows anyway.
const QUIET: Duration = Duration::from_secs(60);

/// How a guide opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opening {
    /// At once: the user asked for it, or the machine was just installed.
    Shown,
    /// Out of sight until the agent answers: closed for good if the machine
    /// is set up already, shown if it is not or does not answer in [`QUIET`].
    Quiet,
}

/// Where a machine's setup stands. Declared in the order the steps come.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Step {
    SignIn,
    Install,
    Accessibility,
    ScreenRecording,
    Save,
    Ready,
}

impl Step {
    /// The steps the guide lists, in order; `Ready` is past the last.
    pub(crate) const LISTED: [Self; 5] = [
        Self::SignIn,
        Self::Install,
        Self::Accessibility,
        Self::ScreenRecording,
        Self::Save,
    ];

    /// The step a machine is on: the agent's answer settles the first four,
    /// and an image kept from it the last.
    fn of(signed_in: bool, permissions: Option<(bool, bool)>, saved: bool) -> Self {
        match permissions {
            _ if saved => Self::Ready,
            None if signed_in => Self::Install,
            None => Self::SignIn,
            Some((false, _)) => Self::Accessibility,
            Some((true, false)) => Self::ScreenRecording,
            Some((true, true)) => Self::Save,
        }
    }
}

#[derive(Default)]
struct Guide {
    /// Which opening of the guide this is: its watcher stops once another
    /// replaced it.
    generation: u64,
    /// The user said they signed in; the agent answering says it too.
    signed_in: bool,
    /// The agent's last answer: Accessibility, then Screen Recording. A
    /// failed ask keeps it, as the agent restarts to take a permission.
    permissions: Option<(bool, bool)>,
    asking: bool,
    /// Not shown yet: see [`Opening::Quiet`].
    quiet: bool,
    /// Opened quietly and not asked for since: the agent's first answer
    /// settles it, even after [`QUIET`] showed it.
    from_image: bool,
    /// Since when a quiet guide has seen its machine running without a
    /// break: a stop or a suspend starts the wait over.
    running_since: Option<Instant>,
    saving: bool,
    saved: Option<Name>,
    failure: Option<String>,
}

/// What a machine's guide shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    pub(crate) step: Step,
    pub(crate) saving: bool,
    /// The image it was kept as, once it was.
    pub(crate) saved: Option<Name>,
    pub(crate) failure: Option<String>,
}

thread_local! {
    static GUIDES: RefCell<HashMap<Name, Guide>> = RefCell::new(HashMap::new());
    static OPENED: Cell<u64> = const { Cell::new(0) };
}

fn with<T>(name: &Name, change: impl FnOnce(&mut Guide) -> T) -> Option<T> {
    GUIDES.with(|guides| guides.borrow_mut().get_mut(name).map(change))
}

/// Machine `name`'s guide, if it is open and not quiet.
pub(crate) fn shown(name: &Name) -> Option<Shown> {
    GUIDES.with(|guides| {
        let guides = guides.borrow();
        let guide = guides.get(name).filter(|guide| !guide.quiet);
        guide.map(|guide| Shown {
            step: Step::of(guide.signed_in, guide.permissions, guide.saved.is_some()),
            saving: guide.saving,
            saved: guide.saved.clone(),
            failure: guide.failure.clone(),
        })
    })
}

/// Opens machine `name`'s guide as `opening` says, and starts asking its
/// agent. Asking for one open quietly shows it.
pub(crate) fn begin(name: &Name, opening: Opening, cx: &mut App) {
    let fresh = GUIDES.with(|guides| {
        let mut guides = guides.borrow_mut();
        if let Some(guide) = guides.get_mut(name) {
            guide.quiet &= opening == Opening::Quiet;
            guide.from_image &= opening == Opening::Quiet;
            return None;
        }
        let generation = OPENED.get() + 1;
        OPENED.set(generation);
        guides.insert(
            name.clone(),
            Guide {
                generation,
                quiet: opening == Opening::Quiet,
                from_image: opening == Opening::Quiet,
                ..Guide::default()
            },
        );
        Some(generation)
    });
    if let Some(generation) = fresh {
        watch(name.clone(), generation, cx);
    }
    cx.refresh_windows();
}

/// Drops machine `name`'s guide, as the machine was deleted: one made later
/// under its name starts over.
pub(crate) fn forget(name: &Name) {
    GUIDES.with(|guides| guides.borrow_mut().remove(name));
}

/// The user signed in; the agent cannot say so before it is installed.
pub(crate) fn signed_in(name: &Name, cx: &mut App) {
    with(name, |guide| guide.signed_in = true);
    cx.refresh_windows();
}

/// Closes machine `name`'s guide, for good: its folder stops asking for it.
pub(crate) fn close(name: &Name, cx: &mut App) {
    GUIDES.with(|guides| guides.borrow_mut().remove(name));
    let name = name.clone();
    cx.background_executor()
        .spawn(async move {
            let removed = store::folder(&name).and_then(|folder| {
                let path = folder.join(store::GUIDE);
                std::fs::remove_file(&path).or_else(|source| match source.kind() {
                    std::io::ErrorKind::NotFound => Ok(()),
                    _ => Err(store::write_error(&path, source)),
                })
            });
            if let Err(error) = removed {
                tracing::warn!(%name, %error, "Cannot forget a machine's setup guide");
            }
        })
        .detach();
    cx.refresh_windows();
}

/// Keeps machine `name`, which must be shut down, as an image named after
/// it: the last step.
pub(crate) fn save(name: &Name, cx: &mut App) {
    let Some(false) = with(name, |guide| std::mem::replace(&mut guide.saving, true)) else {
        return;
    };
    with(name, |guide| guide.failure = None);
    cx.refresh_windows();
    let images = cx.background_executor().spawn(async { image::list() });
    let name = name.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let images = images.await;
        cx.update(|cx| {
            let taken = |text: &str| {
                images
                    .as_ref()
                    .is_ok_and(|images| images.iter().any(|image| image.name.as_str() == text))
            };
            let target = match &images {
                Ok(_) => image::unused(&name, taken),
                Err(error) => return finish(&name, Err(error.localize()), cx),
            };
            let Some(target) = target else {
                return finish(&name, Err(super::Error::InvalidName.localize()), cx);
            };
            let done = name.clone();
            super::save_image(&name, target.clone(), cx, move |saved, cx| {
                finish(&done, saved.map(|()| target).map_err(|e| e.localize()), cx);
            });
        });
    })
    .detach();
}

fn finish(name: &Name, saved: Result<Name, String>, cx: &mut App) {
    with(name, |guide| {
        guide.saving = false;
        match saved {
            Ok(image) => guide.saved = Some(image),
            Err(message) => guide.failure = Some(message),
        }
    });
    cx.refresh_windows();
}

/// Notes off the main thread that machine `name`'s agent is set up.
fn mark_ready(name: &Name, cx: &mut App) {
    let name = name.clone();
    cx.background_executor()
        .spawn(async move {
            if let Err(error) = store::mark_ready(&name) {
                tracing::warn!(%name, %error, "Cannot note that a machine is set up for agents");
            }
        })
        .detach();
}

/// What the agent's answer did to a guide.
#[derive(Debug, PartialEq, Eq)]
enum Answered {
    /// Nothing to show differently.
    Same,
    /// A step moved on, or a quiet guide shows.
    Changed { ready: bool },
    /// A guide opened quietly found its machine set up already: it closes
    /// for good.
    Settled,
}

impl Guide {
    /// Takes the agent's answer, `permissions` when it gave one.
    fn answered(&mut self, permissions: Option<(bool, bool)>) -> Answered {
        let Some(permissions) = permissions else {
            return Answered::Same;
        };
        let ready = permissions == (true, true);
        let quiet = std::mem::take(&mut self.quiet);
        // The first answer of a guide opened quietly speaks for the image,
        // however long the agent took to give it.
        if ready && self.from_image && self.permissions.is_none() {
            return Answered::Settled;
        }
        self.signed_in = true;
        let before = self.permissions.replace(permissions);
        if before == Some(permissions) && !quiet {
            return Answered::Same;
        }
        Answered::Changed {
            ready: ready && before != Some(permissions),
        }
    }

    /// Shows a quiet guide whose machine has run for [`QUIET`] without a
    /// break at `now`, the agent never answering; whether it did.
    fn waited(&mut self, running: bool, now: Instant) -> bool {
        if !running {
            self.running_since = None;
        }
        if !self.quiet || !running {
            return false;
        }
        let since = *self.running_since.get_or_insert(now);
        self.quiet = now.duration_since(since) < QUIET;
        !self.quiet
    }
}

/// Asks machine `name`'s agent where setup stands, every [`EVERY`] while
/// it runs, until its guide closes, opens again anew, or needs the agent no
/// more.
fn watch(name: Name, generation: u64, cx: &mut App) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            let ask = cx.update(|cx| {
                let running = status(&name) == Status::Running;
                let (waiting, shows) = with(&name, |guide| {
                    (guide.generation == generation).then(|| {
                        let shows = guide.waited(running, Instant::now());
                        let waiting = !guide.asking
                            && Step::of(guide.signed_in, guide.permissions, false) < Step::Save;
                        (waiting, shows)
                    })
                })??;
                if shows {
                    cx.refresh_windows();
                }
                Some(waiting && running)
            });
            match ask {
                None => break,
                Some(true) => {
                    let asked = cx.update(|cx| {
                        with(&name, |guide| guide.asking = true);
                        agent::hello(&name, cx)
                    });
                    let answer = asked.await;
                    let permissions = answer
                        .ok()
                        .map(|hello| (hello.accessibility, hello.screen_recording));
                    cx.update(|cx| {
                        let answered = with(&name, |guide| {
                            (guide.generation == generation).then(|| {
                                guide.asking = false;
                                guide.answered(permissions)
                            })
                        });
                        match answered.flatten() {
                            None | Some(Answered::Same) => {}
                            Some(Answered::Changed { ready }) => {
                                if ready {
                                    mark_ready(&name, cx);
                                }
                                cx.refresh_windows();
                            }
                            Some(Answered::Settled) => {
                                tracing::info!(%name, "A machine made from an image was set up already; its guide closes");
                                mark_ready(&name, cx);
                                close(&name, cx);
                            }
                        }
                    });
                }
                Some(false) => {}
            }
            cx.background_executor().timer(EVERY).await;
        }
    })
    .detach();
}

#[cfg(test)]
mod tests;
