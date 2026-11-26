//! What Virtualization does to make a machine: say which macOS this Mac runs
//! best and where Apple keeps it, lay out a new machine's firmware storage,
//! and install macOS onto its disk.
//!
//! The framework answers through completion handlers, some on a thread of
//! its own choosing, so each answer goes into a [`Slot`] the UI thread polls.
//!
//! `objc2-virtualization` marks every call `unsafe`, as it does not check the
//! framework's threading rules. The machine and its installer are made and
//! driven on the main thread, the machine's queue.
#![allow(unsafe_code)]

use super::Error;
use crate::machine::{
    self,
    backend::apple::{description, file_url},
};
use block2::RcBlock;
use objc2::{AnyThread as _, MainThreadMarker, rc::Retained};
use objc2_foundation::{NSData, NSError};
use objc2_virtualization::{
    VZMacAuxiliaryStorage, VZMacAuxiliaryStorageInitializationOptions, VZMacHardwareModel,
    VZMacOSInstaller, VZMacOSRestoreImage, VZVirtualMachine, VZVirtualMachineConfiguration,
};
use std::{
    path::Path,
    ptr::NonNull,
    sync::{Arc, Mutex, PoisonError},
};

/// Where an answer lands, from whichever thread gives it.
pub(super) type Slot<T> = Arc<Mutex<Option<T>>>;

/// Takes what `slot` holds, if anything has landed yet.
pub(super) fn take<T>(slot: &Slot<T>) -> Option<T> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).take()
}

fn fill<T>(slot: &Slot<T>, value: T) {
    slot.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert(value);
}

/// The newest macOS Apple offers this Mac, and what a machine needs to run it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Release {
    pub(super) url: String,
    pub(super) build: String,
    /// As people read it: 26.1, or 15.6.1.
    pub(super) version: String,
    /// The hardware model a machine for it is made with.
    pub(super) hardware: Vec<u8>,
    pub(super) min_cpus: u16,
    pub(super) min_memory_bytes: u64,
}

/// Asks Apple for the newest macOS this Mac can run.
pub(super) fn latest() -> Slot<Result<Release, Error>> {
    let slot: Slot<Result<Release, Error>> = Arc::default();
    let answer = slot.clone();
    let done = RcBlock::new(
        move |image: *mut VZMacOSRestoreImage, error: *mut NSError| {
            // SAFETY: the framework passes null or objects that outlive this call.
            fill(&answer, unsafe { release(image, error) });
        },
    );
    // SAFETY: a class method; the framework copies the block, which only
    // touches the slot, so any thread may call it.
    unsafe { VZMacOSRestoreImage::fetchLatestSupportedWithCompletionHandler(&done) };
    slot
}

/// What the fetched `image` says, or why there is none.
///
/// # Safety
///
/// Each pointer is null or points to an object alive for this call.
unsafe fn release(image: *mut VZMacOSRestoreImage, error: *mut NSError) -> Result<Release, Error> {
    // SAFETY: as the caller promises.
    if let Some(error) = NonNull::new(error) {
        return Err(Error::Fetch(description(unsafe { error.as_ref() })));
    }
    // SAFETY: as the caller promises.
    let image = unsafe { NonNull::new(image).ok_or(Error::NotSupported)?.as_ref() };
    // SAFETY: plain reads of a fetched image's properties.
    unsafe {
        let requirements = image
            .mostFeaturefulSupportedConfiguration()
            .ok_or(Error::NotSupported)?;
        let hardware = requirements.hardwareModel();
        if !hardware.isSupported() {
            return Err(Error::NotSupported);
        }
        let url = image
            .URL()
            .absoluteString()
            .map(|url| url.to_string())
            .ok_or(Error::NotSupported)?;
        let os = image.operatingSystemVersion();
        Ok(Release {
            url,
            build: image.buildVersion().to_string(),
            version: version(os.majorVersion, os.minorVersion, os.patchVersion),
            hardware: hardware.dataRepresentation().to_vec(),
            min_cpus: u16::try_from(requirements.minimumSupportedCPUCount()).unwrap_or(u16::MAX),
            min_memory_bytes: requirements.minimumSupportedMemorySize(),
        })
    }
}

/// A macOS version as Apple writes it, without a zero patch.
pub(super) fn version(major: isize, minor: isize, patch: isize) -> String {
    match patch {
        0 => format!("{major}.{minor}"),         // i18n: data
        _ => format!("{major}.{minor}.{patch}"), // i18n: data
    }
}

/// Lays out the firmware storage of a machine of hardware model `hardware`
/// at `path`.
pub(super) fn create_auxiliary(path: &Path, hardware: &[u8]) -> machine::Result<()> {
    let hardware = NSData::with_bytes(hardware);
    // SAFETY: a value object and a new file, made and used on this thread.
    unsafe {
        let model =
            VZMacHardwareModel::initWithDataRepresentation(VZMacHardwareModel::alloc(), &hardware)
                .filter(|model| model.isSupported())
                .ok_or(machine::Error::Hardware)?;
        VZMacAuxiliaryStorage::initCreatingStorageAtURL_hardwareModel_options_error(
            VZMacAuxiliaryStorage::alloc(),
            &file_url(path),
            &model,
            VZMacAuxiliaryStorageInitializationOptions::AllowOverwrite,
        )
        .map_err(|error| machine::Error::Framework(description(&error)))?;
    }
    Ok(())
}

/// macOS being installed onto a machine that never shows: the installer
/// boots it, writes its disk, and stops it.
pub(super) struct Installing {
    _machine: Retained<VZVirtualMachine>,
    installer: Retained<VZMacOSInstaller>,
    pub(super) done: Slot<Result<(), Error>>,
}

impl Installing {
    /// Installs the restore image at `image` with `configuration`.
    pub(super) fn start(
        configuration: &VZVirtualMachineConfiguration,
        image: &Path,
    ) -> machine::Result<Self> {
        MainThreadMarker::new().ok_or(machine::Error::Unsupported)?;
        let done: Slot<Result<(), Error>> = Arc::default();
        let answer = done.clone();
        let finished = RcBlock::new(move |error: *mut NSError| {
            // SAFETY: the framework passes null or an error that outlives this
            // call.
            let result = match NonNull::new(error) {
                None => Ok(()),
                Some(error) => Err(Error::Install(description(unsafe { error.as_ref() }))),
            };
            fill(&answer, result);
        });
        // SAFETY: on the main thread, so the machine's queue is the main
        // queue, where the installer must be made and started. The
        // configuration was validated, and the image is a local file.
        unsafe {
            let machine =
                VZVirtualMachine::initWithConfiguration(VZVirtualMachine::alloc(), configuration);
            let installer = VZMacOSInstaller::initWithVirtualMachine_restoreImageURL(
                VZMacOSInstaller::alloc(),
                &machine,
                &file_url(image),
            );
            installer.installWithCompletionHandler(&finished);
            Ok(Self {
                _machine: machine,
                installer,
                done,
            })
        }
    }

    /// How far it has come, from 0 to 1.
    pub(super) fn fraction(&self) -> f64 {
        // SAFETY: reads the installer's progress on the main thread.
        let fraction = unsafe { self.installer.progress().fractionCompleted() };
        if fraction.is_finite() {
            fraction.clamp(0., 1.)
        } else {
            0.
        }
    }

    /// Stops it; its completion then reports the cancellation.
    pub(super) fn cancel(&self) {
        // SAFETY: cancelling a started installer's progress is how Apple
        // stops one, on the main thread.
        unsafe { self.installer.progress().cancel() };
    }
}
