//! The setup guide a machine's tab shows beside it: the steps that make it
//! ready for agents (sign in, install Sheprd Agent, allow its two
//! permissions, keep the machine as an image). Sheprd checks each step
//! through the agent and moves on by itself; signing in is the one it cannot
//! see until the agent runs, so the user says when it is done.
//!
//! A machine made from an image starts with its guide open (the `GUIDE`
//! file in its folder), and Settings opens it for any machine. The guide
//! lives here, in memory, so every window showing the machine agrees.

use super::{Name, Status, agent, image, status, store};
use crate::i18n::Localize as _;
use gpui::{App, AsyncApp};
use std::{cell::RefCell, collections::HashMap, time::Duration};

/// How often a waiting step asks the agent again.
const EVERY: Duration = Duration::from_secs(2);

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
    /// The user said they signed in; the agent answering says it too.
    signed_in: bool,
    /// The agent's last answer: Accessibility, then Screen Recording. A
    /// failed ask keeps it, as the agent restarts to take a permission.
    permissions: Option<(bool, bool)>,
    asking: bool,
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
}

fn with<T>(name: &Name, change: impl FnOnce(&mut Guide) -> T) -> Option<T> {
    GUIDES.with(|guides| guides.borrow_mut().get_mut(name).map(change))
}

/// Machine `name`'s guide, if it is open.
pub(crate) fn shown(name: &Name) -> Option<Shown> {
    GUIDES.with(|guides| {
        guides.borrow().get(name).map(|guide| Shown {
            step: Step::of(guide.signed_in, guide.permissions, guide.saved.is_some()),
            saving: guide.saving,
            saved: guide.saved.clone(),
            failure: guide.failure.clone(),
        })
    })
}

/// Opens machine `name`'s guide, and starts asking its agent.
pub(crate) fn begin(name: &Name, cx: &mut App) {
    let fresh = GUIDES.with(|guides| {
        let mut guides = guides.borrow_mut();
        let fresh = !guides.contains_key(name);
        guides.entry(name.clone()).or_default();
        fresh
    });
    if fresh {
        watch(name.clone(), cx);
    }
    cx.refresh_windows();
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

/// Asks machine `name`'s agent where setup stands, every [`EVERY`] while
/// it runs, until its guide closes or needs the agent no more.
fn watch(name: Name, cx: &mut App) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            let ask = cx.update(|_| {
                let waiting = with(&name, |guide| {
                    !guide.asking
                        && Step::of(guide.signed_in, guide.permissions, false) < Step::Save
                })?;
                Some(waiting && status(&name) == Status::Running)
            });
            match ask {
                None => break,
                Some(true) => {
                    let asked = cx.update(|cx| {
                        with(&name, |guide| guide.asking = true);
                        agent::hello(&name, cx)
                    });
                    let answer = asked.await;
                    cx.update(|cx| {
                        let changed = with(&name, |guide| {
                            guide.asking = false;
                            let before = guide.permissions;
                            if let Ok(hello) = &answer {
                                guide.signed_in = true;
                                guide.permissions =
                                    Some((hello.accessibility, hello.screen_recording));
                            }
                            before != guide.permissions
                        });
                        if changed == Some(true) {
                            cx.refresh_windows();
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
