//! Images: a machine set up once, that new machines start from.
//!
//! An image is a machine's disk and firmware storage, kept apart from any
//! machine so it stays as it was saved. Saving one and starting from one clone
//! the files: on APFS a clone shares its blocks with the original, so neither
//! takes real space until it changes.
//!
//! An image has no identity. Each machine made from it gets its own, so two of
//! them can run at once, and with it its own network address. Nothing else on
//! the disk changes, so an account set up in the image is there in each
//! machine, with what it logged in to as far as that survives a new identity.
//!
//! Everything here reaches the disk, so it runs off the main thread.

use super::{
    Error, Name, Result,
    configuration::new_identity,
    store::{self, AUXILIARY, DISK, FROM_IMAGE, GUIDE, HARDWARE, IDENTITY, READY, SETTINGS, STATE},
};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// A name a machine or image was given by hand keeps this much room for the
/// suffix of [`unused`].
const SUFFIX_BYTES: usize = 4;

/// An image as its folder describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub(crate) name: Name,
    /// What its files add up to. They are shared with what was cloned from
    /// it, so deleting it may give back less.
    pub(crate) disk_bytes: u64,
}

/// The folder that holds every image.
fn root() -> Result<PathBuf> {
    let state = crate::preferences::state_dir().ok_or(Error::NoStateDir)?;
    Ok(state.join("machine-images"))
}

/// Every image, by name.
pub(crate) fn list() -> Result<Vec<Image>> {
    list_in(&root()?)
}

fn list_in(root: &Path) -> Result<Vec<Image>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(store::read_error(root, source)),
    };
    let mut images: Vec<Image> = entries
        .flatten()
        .filter_map(|entry| {
            let name = Name::try_from(entry.file_name().into_string().ok()?).ok()?;
            let folder = entry.path();
            folder.join(DISK).is_file().then(|| Image {
                name,
                disk_bytes: store::disk_bytes(&folder),
            })
        })
        .collect();
    images.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    Ok(images)
}

/// Keeps stopped machine `machine` as image `image`. The caller holds it, so
/// that nothing starts it meanwhile.
pub(super) fn save(machine: &Name, image: &Name) -> Result<()> {
    save_in(&store::folder(machine)?, machine, &root()?, image)
}

fn save_in(source: &Path, machine: &Name, root: &Path, image: &Name) -> Result<()> {
    if !source.join(DISK).is_file() {
        return Err(Error::NotInstalled(source.to_owned()));
    }
    // Its disk only makes sense with the memory saved beside it.
    if source.join(STATE).is_file() {
        return Err(Error::Suspended(machine.clone()));
    }
    clone_as(source, root, image, |_| Ok(()))
}

/// Makes machine `name` from image `image`, with an identity of its own. An
/// image whose agent was seen set up makes a machine without a setup guide;
/// one saved before Sheprd kept that note opens its guide once the agent
/// says it is needed ([`super::guide::Opening::Quiet`]).
pub(crate) fn create(name: &Name, image: &Name) -> Result<()> {
    create_in(
        &root()?.join(image.as_str()),
        image,
        &store::root()?,
        name,
        &new_identity(),
    )
}

fn create_in(source: &Path, image: &Name, root: &Path, name: &Name, identity: &[u8]) -> Result<()> {
    if !source.join(DISK).is_file() {
        return Err(Error::ImageNotFound(image.clone()));
    }
    let ready = source.join(READY).is_file();
    clone_as(source, root, name, |staging| {
        let path = staging.join(IDENTITY);
        fs::write(&path, identity).map_err(|source| store::write_error(&path, source))?;
        if ready {
            return Ok(());
        }
        let guide = staging.join(GUIDE);
        fs::write(&guide, FROM_IMAGE).map_err(|source| store::write_error(&guide, source))
    })
}

/// Deletes image `image`. What was made from it is not touched.
pub(crate) fn delete(image: &Name) -> Result<()> {
    store::delete_folder(&root()?.join(image.as_str()))
}

/// Clones what machines and images share into `root/name`, which appears
/// whole or not at all: it is built under a name no listing reads.
fn clone_as(
    source: &Path,
    root: &Path,
    name: &Name,
    finish: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let target = root.join(name.as_str());
    if target.exists() {
        return Err(Error::Exists(name.clone()));
    }
    fs::create_dir_all(root).map_err(|source| store::write_error(root, source))?;
    let staging = tempfile::Builder::new()
        .prefix(".new-")
        .tempdir_in(root)
        .map_err(|source| store::write_error(root, source))?;
    for file in [AUXILIARY, DISK, HARDWARE] {
        clone_file(&source.join(file), &staging.path().join(file))?;
    }
    for file in [READY, SETTINGS] {
        match clone_file(&source.join(file), &staging.path().join(file)) {
            Err(Error::Read { source, .. }) if source.kind() == ErrorKind::NotFound => {}
            cloned => cloned?,
        }
    }
    finish(staging.path())?;
    fs::rename(staging.path(), &target).map_err(|source| match source.kind() {
        ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty => Error::Exists(name.clone()),
        _ => store::write_error(&target, source),
    })
}

/// `fs::copy` clones on APFS, and copies where it cannot.
fn clone_file(from: &Path, to: &Path) -> Result<()> {
    if let Err(source) = fs::metadata(from) {
        return Err(store::read_error(from, source));
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|source| store::write_error(to, source))
}

/// `base`, or `base-2` and on, the first of them `taken` does not know.
pub(crate) fn unused(base: &Name, taken: impl Fn(&str) -> bool) -> Option<Name> {
    if !taken(base.as_str()) {
        return Some(base.clone());
    }
    let room = super::MAX_NAME_BYTES - SUFFIX_BYTES;
    // Names are ASCII, so any byte is a boundary.
    let stem = &base.as_str()[..base.as_str().len().min(room)];
    (2..1000)
        .map(|number| format!("{stem}-{number}")) // i18n: data
        .find(|name| !taken(name))
        .and_then(|name| Name::try_from(name).ok())
}

#[cfg(test)]
mod tests;
