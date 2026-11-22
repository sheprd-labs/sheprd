//! Making a virtual Mac from nothing: Apple's newest macOS for this Mac is
//! downloaded and installed onto a new machine's disk.
//!
//! One install runs at a time, owned by the app like the machines it makes,
//! so closing Settings leaves it running and reopening shows where it is.
//! The machine is built in a folder no listing reads and appears whole once
//! macOS is on it, opening with its setup guide. The download is kept until
//! then, so a failed or stopped try resumes it rather than starting over;
//! it is deleted once a machine is made, as it is tens of gigabytes.

mod apple;
mod download;
mod error;

pub use error::Error;

use super::{
    Limits, Name, Result, Settings,
    configuration::{Bundle, new_identity},
    store::{self, AUXILIARY, DISK, GUIDE, HARDWARE, IDENTITY},
};
use crate::window::SheprdWindow;
use apple::{Installing, Release, Slot};
use download::Counter;
use gpui::{App, AsyncApp, WeakEntity};
use std::{
    cell::RefCell,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::{Duration, SystemTime},
};

/// How often progress is read and drawn.
const TICK: Duration = Duration::from_secs(1);
/// What a new machine's disk can grow to. The file is sparse: it takes only
/// what macOS and its files fill.
const DISK_BYTES: u64 = 64 << 30;
/// A staging folder this old was left by an install that never finished,
/// as when the app quit during one.
const ABANDONED: Duration = Duration::from_secs(60 * 60);
const STAGING: &str = ".install-";

/// Where an install has got to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Progress {
    /// Asking Apple which macOS this Mac runs best.
    Fetching,
    Downloading {
        version: String,
        done: u64,
        /// Zero until Apple's server says.
        total: u64,
    },
    /// Laying out the machine's files.
    Preparing {
        version: String,
    },
    Installing {
        version: String,
        fraction: f64,
    },
    /// Why it stopped, for the user. The download is kept for another try.
    Failed(String),
}

struct Current {
    name: Name,
    progress: Progress,
    counter: Arc<Counter>,
    installing: Option<Installing>,
}

thread_local! {
    /// The install under way, the main thread's own as the machines are.
    static CURRENT: RefCell<Option<Current>> = const { RefCell::new(None) };
}

/// The install under way or the last one that failed: which machine, and
/// where it is.
pub(crate) fn progress() -> Option<(Name, Progress)> {
    CURRENT.with_borrow(|current| {
        current
            .as_ref()
            .map(|current| (current.name.clone(), current.progress.clone()))
    })
}

/// Whether machine `name` is being made, so nothing else takes its name.
pub(crate) fn making(name: &Name) -> bool {
    progress().is_some_and(|(making, progress)| {
        making == *name && !matches!(progress, Progress::Failed(_))
    })
}

/// Starts making machine `name` from Apple's newest macOS for this Mac, and
/// opens it in window `source` once made. Returns `false` while another
/// install runs.
pub(crate) fn begin(name: Name, source: WeakEntity<SheprdWindow>, cx: &mut App) -> bool {
    let busy = CURRENT.with_borrow(|current| {
        current
            .as_ref()
            .is_some_and(|current| !matches!(current.progress, Progress::Failed(_)))
    });
    if busy {
        return false;
    }
    let counter = Arc::new(Counter::default());
    CURRENT.set(Some(Current {
        name: name.clone(),
        progress: Progress::Fetching,
        counter: counter.clone(),
        installing: None,
    }));
    cx.refresh_windows();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let made = run(&name, &counter, cx).await;
        cx.update(|cx| {
            match made {
                Ok(()) => {
                    CURRENT.set(None);
                    crate::browser::open_machine_in(&source, name.clone(), cx);
                }
                Err(error) => {
                    tracing::warn!(%name, %error, "Cannot make a machine");
                    let message = crate::i18n::Localize::localize(&error);
                    set(|current| {
                        current.installing = None;
                        current.progress = Progress::Failed(message);
                    });
                }
            }
            super::known::refresh(cx);
            cx.refresh_windows();
        });
    })
    .detach();
    true
}

/// Stops the install under way. It ends as failed, and keeps its download.
pub(crate) fn cancel() {
    CURRENT.with_borrow(|current| {
        if let Some(current) = current {
            current.counter.cancel.store(true, Ordering::Relaxed);
            if let Some(installing) = &current.installing {
                installing.cancel();
            }
        }
    });
}

/// Forgets a failed install, so its row goes.
pub(crate) fn dismiss(cx: &mut App) {
    CURRENT.with_borrow_mut(|current| {
        if current
            .as_ref()
            .is_some_and(|current| matches!(current.progress, Progress::Failed(_)))
        {
            *current = None;
        }
    });
    cx.refresh_windows();
}

/// Shows `progress` for machine `name` as if an install had got there, for
/// tests of what draws it.
#[cfg(test)]
pub(crate) fn pretend(name: Name, progress: Progress) {
    CURRENT.set(Some(Current {
        name,
        progress,
        counter: Arc::default(),
        installing: None,
    }));
}

fn set(change: impl FnOnce(&mut Current)) {
    CURRENT.with_borrow_mut(|current| {
        if let Some(current) = current {
            change(current);
        }
    });
}

fn cancelled(counter: &Counter) -> Result<()> {
    if counter.cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled.into());
    }
    Ok(())
}

/// Polls `slot` until something lands, calling `tick` between looks.
async fn settle<T>(slot: &Slot<T>, cx: &mut AsyncApp, mut tick: impl FnMut(&mut App)) -> T {
    loop {
        if let Some(value) = apple::take(slot) {
            return value;
        }
        cx.update(|cx| tick(cx));
        cx.background_executor().timer(TICK).await;
    }
}

async fn run(name: &Name, counter: &Arc<Counter>, cx: &mut AsyncApp) -> Result<()> {
    // Nothing waits on the answer but this, so Stop need not wait for it.
    let asked = apple::latest();
    let release = loop {
        if let Some(answer) = apple::take(&asked) {
            break answer?;
        }
        cancelled(counter)?;
        cx.background_executor().timer(TICK).await;
    };
    cancelled(counter)?;
    let image = downloads()?.join(format!("{}.ipsw", release.build)); // i18n: data
    let fetched: Slot<std::result::Result<(), Error>> = Arc::default();
    {
        let (url, path, counter, fetched) = (
            release.url.clone(),
            image.clone(),
            counter.clone(),
            fetched.clone(),
        );
        std::thread::Builder::new()
            .name("machine-download".into())
            .spawn(move || {
                forget_other_downloads(&path);
                let result = download::fetch(
                    &download::agent(),
                    &url,
                    &path,
                    download::INSTALL_ROOM,
                    &counter,
                );
                *fetched
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
            })
            .map_err(|source| download::file_error(&image, source))?;
    }
    let version = release.version.clone();
    settle(&fetched, cx, |cx| {
        set(|current| {
            current.progress = Progress::Downloading {
                version: version.clone(),
                done: counter.done.load(Ordering::Relaxed),
                total: counter.total.load(Ordering::Relaxed),
            };
        });
        cx.refresh_windows();
    })
    .await?;
    cancelled(counter)?;
    cx.update(|cx| {
        set(|current| {
            current.progress = Progress::Preparing {
                version: version.clone(),
            }
        });
        cx.refresh_windows();
    });
    let staging = {
        let (name, release) = (name.clone(), release.clone());
        cx.background_executor()
            .spawn(async move { prepare(&name, &release) })
            .await?
    };
    let folder = staging.path().to_owned();
    // A bundle is read only with its firmware storage in place.
    cx.update(|_| apple::create_auxiliary(&folder.join(AUXILIARY), &release.hardware))?;
    let bundle = {
        let folder = folder.clone();
        cx.background_executor()
            .spawn(async move { Bundle::read_in(&folder, false) })
            .await?
    };
    let done = cx.update(|cx| -> Result<_> {
        cancelled(counter)?;
        let installing = Installing::start(&*bundle.configuration()?, &image)?;
        let done = installing.done.clone();
        set(|current| current.installing = Some(installing));
        cx.refresh_windows();
        Ok(done)
    })?;
    settle(&done, cx, |cx| {
        let fraction = CURRENT.with_borrow(|current| {
            current
                .as_ref()
                .and_then(|current| current.installing.as_ref())
                .map_or(0., Installing::fraction)
        });
        set(|current| {
            current.progress = Progress::Installing {
                version: version.clone(),
                fraction,
            };
        });
        cx.refresh_windows();
    })
    .await
    .map_err(|error| match counter.cancel.load(Ordering::Relaxed) {
        true => Error::Cancelled,
        false => error,
    })?;
    cx.update(|_| set(|current| current.installing = None));
    let name = name.clone();
    cx.background_executor()
        .spawn(async move { finish(staging, &name, &image) })
        .await
}

/// The folder downloads wait in, beside the machines.
fn downloads() -> Result<PathBuf> {
    let state = crate::preferences::state_dir().ok_or(super::Error::NoStateDir)?;
    Ok(state.join("machine-downloads"))
}

/// Deletes downloads of other macOS builds than `image`'s, which nothing will
/// resume now that Apple offers a newer one.
fn forget_other_downloads(image: &Path) {
    let (Some(folder), Some(keep)) = (image.parent(), image.file_name()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let partial = download::partial(image);
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name() != Some(keep) && path != partial {
            let _ = fs::remove_file(path);
        }
    }
}

/// The settings a new machine starts with: the defaults, within what this
/// Mac allows and above what macOS needs.
fn first_settings(release: &Release, limits: &Limits) -> Settings {
    let defaults = Settings::default();
    let min_memory_gib =
        u16::try_from(release.min_memory_bytes.div_ceil(1 << 30)).unwrap_or(u16::MAX);
    Settings {
        cpus: defaults
            .cpus
            .clamp(*limits.cpus.start(), *limits.cpus.end())
            .max(release.min_cpus),
        memory_gib: defaults
            .memory_gib
            .clamp(*limits.memory_gib.start(), *limits.memory_gib.end())
            .max(min_memory_gib),
        display: defaults.display,
    }
}

/// Lays out machine `name`'s files, but for its firmware storage, in a folder
/// no listing reads. It is deleted if the install does not finish.
fn prepare(name: &Name, release: &Release) -> Result<tempfile::TempDir> {
    let root = store::root()?;
    if root.join(name.as_str()).exists() {
        return Err(super::Error::Exists(name.clone()));
    }
    fs::create_dir_all(&root).map_err(|source| store::write_error(&root, source))?;
    forget_abandoned(&root);
    let staging = tempfile::Builder::new()
        .prefix(STAGING)
        .tempdir_in(&root)
        .map_err(|source| store::write_error(&root, source))?;
    let folder = staging.path();
    let write = |file: &str, bytes: &[u8]| {
        let path = folder.join(file);
        fs::write(&path, bytes).map_err(|source| store::write_error(&path, source))
    };
    write(HARDWARE, &release.hardware)?;
    write(IDENTITY, &new_identity())?;
    write(GUIDE, b"")?;
    store::write_settings(folder, &first_settings(release, &Limits::host()))?;
    let disk = folder.join(DISK);
    fs::File::create(&disk)
        .and_then(|file| file.set_len(DISK_BYTES))
        .map_err(|source| store::write_error(&disk, source))?;
    Ok(staging)
}

/// Deletes staging folders an install left when the app quit during it.
fn forget_abandoned(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let staged = entry.file_name().to_string_lossy().starts_with(STAGING);
        let idle = fs::metadata(path.join(DISK))
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_none_or(|idle| idle > ABANDONED);
        if staged && idle {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// Puts the installed machine where listings find it, and deletes the
/// download it came from.
fn finish(staging: tempfile::TempDir, name: &Name, image: &Path) -> Result<()> {
    let target = store::folder(name)?;
    let source = staging.path().to_owned();
    fs::rename(&source, &target).map_err(|error| match error.kind() {
        ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty => {
            super::Error::Exists(name.clone())
        }
        _ => store::write_error(&target, error),
    })?;
    // Moved, so there is nothing left for it to delete.
    let _ = staging.keep();
    if let Err(error) = fs::remove_file(image) {
        tracing::warn!(path = %image.display(), %error, "Cannot delete the macOS download");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
