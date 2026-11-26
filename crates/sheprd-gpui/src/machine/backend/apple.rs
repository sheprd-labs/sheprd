//! Apple Virtualization.framework backend for macOS and Linux ARM64 guests.
//!
//! `objc2-virtualization` marks every call `unsafe`, as it does not check
//! the framework's threading rules. Live VMs and views live on the main
//! thread (the machine's queue); configuration objects are plain value/builder
//! objects constructed on the calling thread.
#![allow(unsafe_code)]

use super::StopReason;
use crate::machine::{
    Display, Error, Name, Result, agent,
    configuration::{
        BootFiles, Bundle, FIT_HEIGHT, FIT_WIDTH, LINUX_DEFAULT_HEIGHT, LINUX_DEFAULT_WIDTH,
        mac_address,
    },
    metadata::BootMetadata,
    vm,
};
use block2::RcBlock;
use gpui::{App, AsyncApp, Task};
use objc2::{
    AnyThread as _, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_foundation::{NSArray, NSData, NSError, NSString, NSURL};
use objc2_virtualization::{
    VZDiskImageStorageDeviceAttachment, VZEFIBootLoader, VZEFIVariableStore,
    VZEFIVariableStoreInitializationOptions, VZFileSerialPortAttachment,
    VZGenericMachineIdentifier, VZGenericPlatformConfiguration, VZMACAddress,
    VZMacAuxiliaryStorage, VZMacGraphicsDeviceConfiguration, VZMacGraphicsDisplayConfiguration,
    VZMacHardwareModel, VZMacMachineIdentifier, VZMacOSBootLoader, VZMacPlatformConfiguration,
    VZNATNetworkDeviceAttachment, VZUSBKeyboardConfiguration, VZUSBMassStorageDeviceConfiguration,
    VZUSBScreenCoordinatePointingDeviceConfiguration, VZVirtioBlockDeviceConfiguration,
    VZVirtioConsoleDeviceSerialPortConfiguration, VZVirtioEntropyDeviceConfiguration,
    VZVirtioGraphicsDeviceConfiguration, VZVirtioGraphicsScanoutConfiguration,
    VZVirtioNetworkDeviceConfiguration, VZVirtioSocketConnection, VZVirtioSocketDevice,
    VZVirtioSocketDeviceConfiguration, VZVirtualMachine, VZVirtualMachineConfiguration,
    VZVirtualMachineDelegate, VZVirtualMachineView,
};
use std::{
    cell::Cell,
    os::fd::{BorrowedFd, OwnedFd},
    path::Path,
    ptr::NonNull,
    rc::Rc,
    time::{Duration, Instant},
};

/// A 2x Retina mode for macOS displays, so text stays sharp.
const DISPLAY_PPI: isize = 220;
const VSOCK_POLL: Duration = Duration::from_millis(20);

/// Native GPUI-embeddable view displaying a live Apple VM.
pub(crate) type View = Retained<VZVirtualMachineView>;

define_class!(
    /// Hears a machine stop without being asked: the guest shut itself down,
    /// or the machine failed.
    // SAFETY: NSObject may be subclassed on the main thread, which
    // `MainThreadOnly` holds every use to, and `Watcher` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SheprdMachineWatcher"]
    #[ivars = Name]
    struct Watcher;

    unsafe impl NSObjectProtocol for Watcher {}

    unsafe impl VZVirtualMachineDelegate for Watcher {
        #[unsafe(method(guestDidStopVirtualMachine:))]
        fn guest_did_stop(&self, _: &VZVirtualMachine) {
            vm::on_backend_stopped(self.ivars().clone(), StopReason::GuestStopped);
        }

        #[unsafe(method(virtualMachine:didStopWithError:))]
        fn did_stop(&self, _: &VZVirtualMachine, error: &NSError) {
            vm::on_backend_stopped(self.ivars().clone(), StopReason::Failed(description(error)));
        }
    }
);

impl Watcher {
    fn new(name: Name, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(name);
        // SAFETY: NSObject's initializer, on an instance with its ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

/// One live Apple Virtualization.framework VM and its delegate.
#[derive(Clone)]
pub(crate) struct AppleVm {
    machine: Retained<VZVirtualMachine>,
    _watcher: Retained<Watcher>,
}

impl AppleVm {
    /// Builds and wires a stopped `VZVirtualMachine` from `bundle` on the main thread.
    pub(crate) fn create(name: Name, bundle: &Bundle) -> Result<Self> {
        let mtm = MainThreadMarker::new().ok_or(Error::Unsupported)?;
        // SAFETY: a class property.
        if !unsafe { VZVirtualMachine::isSupported() } {
            return Err(Error::Unsupported);
        }
        let configuration = build_configuration(bundle)?;
        // SAFETY: on the main thread, so the machine's queue is the main queue,
        // which Apple's view requires. The configuration was validated.
        let machine = unsafe {
            VZVirtualMachine::initWithConfiguration(VZVirtualMachine::alloc(), &configuration)
        };
        let watcher = Watcher::new(name, mtm);
        // SAFETY: `AppleVm` keeps the delegate alive, which the machine holds weakly.
        unsafe { machine.setDelegate(Some(ProtocolObject::from_ref(&*watcher))) };
        Ok(Self {
            machine,
            _watcher: watcher,
        })
    }

    pub(crate) fn start(
        &self,
        app: AsyncApp,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        let block = once(app, done);
        // SAFETY: on the machine's queue; the framework copies the block.
        unsafe { self.machine.startWithCompletionHandler(&block) };
    }

    pub(crate) fn restore(
        &self,
        state: &Path,
        app: AsyncApp,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        let url = file_url(state);
        let block = once(app, done);
        // SAFETY: on the machine's queue; the framework copies the block.
        unsafe {
            self.machine
                .restoreMachineStateFromURL_completionHandler(&url, &block)
        };
    }

    pub(crate) fn resume(
        &self,
        app: AsyncApp,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        let block = once(app, done);
        // SAFETY: on the machine's queue; the framework copies the block.
        unsafe { self.machine.resumeWithCompletionHandler(&block) };
    }

    pub(crate) fn pause(
        &self,
        app: AsyncApp,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        let block = once(app, done);
        // SAFETY: on the machine's queue; the framework copies the block.
        unsafe { self.machine.pauseWithCompletionHandler(&block) };
    }

    pub(crate) fn save_state(
        &self,
        state: &Path,
        app: AsyncApp,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        let url = file_url(state);
        let block = once(app, done);
        // SAFETY: on the machine's queue; the framework copies the block.
        unsafe {
            self.machine
                .saveMachineStateToURL_completionHandler(&url, &block)
        };
    }

    pub(crate) fn stop(
        &self,
        app: AsyncApp,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        let block = once(app, done);
        // SAFETY: on the machine's queue; the framework copies the block.
        unsafe { self.machine.stopWithCompletionHandler(&block) };
    }

    pub(crate) fn request_stop(&self) -> Result<()> {
        // SAFETY: on the machine's queue.
        unsafe { self.machine.requestStopWithError() }
            .map_err(|error| Error::Framework(description(&error)))
    }

    /// Opens a host-to-guest vsock stream on `port`.
    pub(crate) fn connect_vsock(
        &self,
        name: &Name,
        port: u32,
        cx: &mut App,
    ) -> Task<agent::Result<OwnedFd>> {
        // SAFETY: on the main thread, the machine's queue.
        let device = unsafe { self.machine.socketDevices() }
            .firstObject()
            .and_then(|device| device.downcast::<VZVirtioSocketDevice>().ok());
        let Some(device) = device else {
            return Task::ready(Err(agent::Error::NotRunning(name.clone())));
        };
        let answer: Rc<Cell<Option<agent::Result<OwnedFd>>>> = Rc::default();
        let block = {
            let answer = answer.clone();
            let name = name.clone();
            RcBlock::new(
                move |connection: *mut VZVirtioSocketConnection, error: *mut NSError| {
                    answer.set(Some(opened_vsock(&name, connection, error)));
                },
            )
        };
        // SAFETY: on the machine's queue; the framework copies the block and calls it there.
        unsafe { device.connectToPort_completionHandler(port, &block) };
        let name = name.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let start = Instant::now();
            loop {
                if let Some(answer) = answer.take() {
                    return answer;
                }
                if start.elapsed() > agent::CONNECT {
                    return Err(agent::Error::Unreachable {
                        name,
                        reason: "no answer".to_owned(), // i18n: data
                    });
                }
                cx.background_executor().timer(VSOCK_POLL).await;
            }
        })
    }
}

/// Creates a new unattached `VZVirtualMachineView` on the main thread.
pub(crate) fn create_view() -> Result<View> {
    let mtm = MainThreadMarker::new().ok_or(Error::Unsupported)?;
    // SAFETY: an unattached view, made and set up on the main thread. The
    // host's own Cmd-Tab and Spotlight stay the host's.
    Ok(unsafe {
        let view = VZVirtualMachineView::new(mtm);
        view.setCapturesSystemKeys(false);
        view
    })
}

/// Attaches `view` to `vm`, enabling dynamic display resize only when requested
/// and supported by the guest configuration.
pub(crate) fn attach_view(view: &VZVirtualMachineView, vm: &AppleVm, auto_resize: bool) {
    // SAFETY: the view and the machine are both on the main thread.
    unsafe {
        view.setAutomaticallyReconfiguresDisplay(auto_resize);
        view.setVirtualMachine(Some(&vm.machine));
    }
}

/// Detaches `view` from its virtual machine.
pub(crate) fn detach_view(view: &VZVirtualMachineView) {
    // SAFETY: on the main thread, where the view lives.
    unsafe { view.setVirtualMachine(None) };
}

/// Generates a fresh machine identifier appropriate for `boot`.
pub(crate) fn new_identity(boot: &BootMetadata) -> Vec<u8> {
    // SAFETY: plain value objects, made and read on this thread.
    unsafe {
        match boot {
            BootMetadata::AppleMacos => VZMacMachineIdentifier::new().dataRepresentation().to_vec(),
            BootMetadata::AppleLinuxEfi { .. } => VZGenericMachineIdentifier::new()
                .dataRepresentation()
                .to_vec(),
        }
    }
}

/// Creates a new EFI variable store file at `path` for an ARM64 Linux EFI VM.
pub(crate) fn create_efi_variable_store(path: &Path) -> Result<()> {
    // SAFETY: creates a new EFI variable store value/file on the calling thread.
    unsafe {
        VZEFIVariableStore::initCreatingVariableStoreAtURL_options_error(
            VZEFIVariableStore::alloc(),
            &file_url(path),
            VZEFIVariableStoreInitializationOptions::AllowOverwrite,
        )
        .map_err(|error| Error::Framework(description(&error)))?;
    }
    Ok(())
}

/// Builds and validates a `VZVirtualMachineConfiguration` for `bundle`.
pub(crate) fn build_configuration(
    bundle: &Bundle,
) -> Result<Retained<VZVirtualMachineConfiguration>> {
    match &bundle.boot {
        BootFiles::Macos {
            hardware,
            identity,
            auxiliary,
            installer,
        } => build_macos_configuration(bundle, hardware, identity, auxiliary, installer.as_deref()),
        BootFiles::LinuxEfi {
            identity,
            efi_vars,
            seed_iso,
            serial_log,
        } => build_linux_efi_configuration(
            bundle,
            identity,
            efi_vars,
            seed_iso.as_deref(),
            serial_log.as_deref(),
        ),
    }
}

fn build_macos_configuration(
    bundle: &Bundle,
    hardware: &[u8],
    identity: &[u8],
    auxiliary: &Path,
    installer: Option<&Path>,
) -> Result<Retained<VZVirtualMachineConfiguration>> {
    let hardware = NSData::with_bytes(hardware);
    let identity_data = NSData::with_bytes(identity);
    let auxiliary = file_url(auxiliary);
    let disk = file_url(&bundle.disk);
    let (width, height) = match bundle.settings.display {
        Display::Fit => (FIT_WIDTH, FIT_HEIGHT),
        Display::Fixed { width, height } => (width, height),
    };
    // SAFETY: plain configuration objects, built and used on this thread.
    // Each init is given the object its class requires.
    unsafe {
        let hardware =
            VZMacHardwareModel::initWithDataRepresentation(VZMacHardwareModel::alloc(), &hardware)
                .filter(|model| model.isSupported())
                .ok_or(Error::Hardware)?;
        let machine_id = VZMacMachineIdentifier::initWithDataRepresentation(
            VZMacMachineIdentifier::alloc(),
            &identity_data,
        )
        .ok_or(Error::Hardware)?;
        let platform = VZMacPlatformConfiguration::new();
        platform.setHardwareModel(&hardware);
        platform.setMachineIdentifier(&machine_id);
        let auxiliary =
            VZMacAuxiliaryStorage::initWithURL(VZMacAuxiliaryStorage::alloc(), &auxiliary);
        platform.setAuxiliaryStorage(Some(&auxiliary));

        let attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
            VZDiskImageStorageDeviceAttachment::alloc(),
            &disk,
            false,
        )
        .map_err(|error| Error::Framework(description(&error)))?;
        let block = VZVirtioBlockDeviceConfiguration::initWithAttachment(
            VZVirtioBlockDeviceConfiguration::alloc(),
            &attachment,
        );

        let display =
            VZMacGraphicsDisplayConfiguration::initWithWidthInPixels_heightInPixels_pixelsPerInch(
                VZMacGraphicsDisplayConfiguration::alloc(),
                isize::try_from(width).unwrap_or(isize::MAX),
                isize::try_from(height).unwrap_or(isize::MAX),
                DISPLAY_PPI,
            );
        let graphics = VZMacGraphicsDeviceConfiguration::new();
        graphics.setDisplays(&NSArray::from_retained_slice(&[display]));

        let network = build_network(identity)?;

        let configuration = VZVirtualMachineConfiguration::new();
        let boot = VZMacOSBootLoader::new();
        configuration.setBootLoader(Some(&boot));
        configuration.setCPUCount(bundle.settings.cpus.into());
        configuration.setMemorySize(u64::from(bundle.settings.memory_gib) << 30);
        configuration.setPlatform(&platform);

        let mut storage = vec![Retained::into_super(block)];
        if let Some(installer) = installer {
            // A USB disk, so the guest shows it on the desktop as one plugged
            // in. Read-only: nothing in the guest changes it.
            let attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
                VZDiskImageStorageDeviceAttachment::alloc(),
                &file_url(installer),
                true,
            )
            .map_err(|error| Error::Framework(description(&error)))?;
            storage.push(Retained::into_super(
                VZUSBMassStorageDeviceConfiguration::initWithAttachment(
                    VZUSBMassStorageDeviceConfiguration::alloc(),
                    &attachment,
                ),
            ));
        }
        configuration.setStorageDevices(&NSArray::from_retained_slice(&storage));
        configuration.setGraphicsDevices(&NSArray::from_retained_slice(&[Retained::into_super(
            graphics,
        )]));
        attach_common_devices(&configuration, network);
        configuration
            .validateWithError()
            .map_err(|error| Error::Framework(description(&error)))?;
        configuration
            .validateSaveRestoreSupportWithError()
            .map_err(|error| Error::Framework(description(&error)))?;
        Ok(configuration)
    }
}

fn build_linux_efi_configuration(
    bundle: &Bundle,
    identity: &[u8],
    efi_vars: &Path,
    seed_iso: Option<&Path>,
    serial_log: Option<&Path>,
) -> Result<Retained<VZVirtualMachineConfiguration>> {
    let identity_data = NSData::with_bytes(identity);
    let efi_url = file_url(efi_vars);
    let disk = file_url(&bundle.disk);
    let (width, height) = match bundle.settings.display {
        Display::Fit => (LINUX_DEFAULT_WIDTH, LINUX_DEFAULT_HEIGHT),
        Display::Fixed { width, height } => (width, height),
    };
    // SAFETY: plain configuration objects, built and used on this thread.
    unsafe {
        let machine_id = VZGenericMachineIdentifier::initWithDataRepresentation(
            VZGenericMachineIdentifier::alloc(),
            &identity_data,
        )
        .ok_or(Error::Hardware)?;
        let platform = VZGenericPlatformConfiguration::new();
        platform.setMachineIdentifier(&machine_id);

        let variable_store = VZEFIVariableStore::initWithURL(VZEFIVariableStore::alloc(), &efi_url);
        let boot = VZEFIBootLoader::new();
        boot.setVariableStore(Some(&variable_store));

        let attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
            VZDiskImageStorageDeviceAttachment::alloc(),
            &disk,
            false,
        )
        .map_err(|error| Error::Framework(description(&error)))?;
        let block = VZVirtioBlockDeviceConfiguration::initWithAttachment(
            VZVirtioBlockDeviceConfiguration::alloc(),
            &attachment,
        );

        let mut storage = vec![Retained::into_super(block)];
        if let Some(seed_path) = seed_iso {
            let seed_attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
                VZDiskImageStorageDeviceAttachment::alloc(),
                &file_url(seed_path),
                true,
            )
            .map_err(|error| Error::Framework(description(&error)))?;
            storage.push(Retained::into_super(
                VZVirtioBlockDeviceConfiguration::initWithAttachment(
                    VZVirtioBlockDeviceConfiguration::alloc(),
                    &seed_attachment,
                ),
            ));
        }

        let scanout = VZVirtioGraphicsScanoutConfiguration::initWithWidthInPixels_heightInPixels(
            VZVirtioGraphicsScanoutConfiguration::alloc(),
            isize::try_from(width).unwrap_or(isize::MAX),
            isize::try_from(height).unwrap_or(isize::MAX),
        );
        let graphics = VZVirtioGraphicsDeviceConfiguration::new();
        graphics.setScanouts(&NSArray::from_retained_slice(&[scanout]));

        let network = build_network(identity)?;

        let configuration = VZVirtualMachineConfiguration::new();
        configuration.setBootLoader(Some(&boot));
        configuration.setCPUCount(bundle.settings.cpus.into());
        configuration.setMemorySize(u64::from(bundle.settings.memory_gib) << 30);
        configuration.setPlatform(&platform);
        configuration.setStorageDevices(&NSArray::from_retained_slice(&storage));
        configuration.setGraphicsDevices(&NSArray::from_retained_slice(&[Retained::into_super(
            graphics,
        )]));
        if let Some(serial_path) = serial_log {
            let attachment = VZFileSerialPortAttachment::initWithURL_append_error(
                VZFileSerialPortAttachment::alloc(),
                &file_url(serial_path),
                true,
            )
            .map_err(|error| Error::Framework(description(&error)))?;
            let serial = VZVirtioConsoleDeviceSerialPortConfiguration::new();
            serial.setAttachment(Some(&attachment));
            configuration.setSerialPorts(&NSArray::from_retained_slice(&[Retained::into_super(
                serial,
            )]));
        }
        attach_common_devices(&configuration, network);
        configuration
            .validateWithError()
            .map_err(|error| Error::Framework(description(&error)))?;
        Ok(configuration)
    }
}

/// # Safety
/// Must be called on the configuration-building thread with valid objects.
unsafe fn build_network(identity: &[u8]) -> Result<Retained<VZVirtioNetworkDeviceConfiguration>> {
    // SAFETY: plain configuration objects created and configured on this thread.
    unsafe {
        let network = VZVirtioNetworkDeviceConfiguration::new();
        let address = NSString::from_str(&mac_address(identity));
        let address =
            VZMACAddress::initWithString(VZMACAddress::alloc(), &address).ok_or(Error::Hardware)?;
        network.setMACAddress(&address);
        let nat = VZNATNetworkDeviceAttachment::new();
        network.setAttachment(Some(&nat));
        Ok(network)
    }
}

/// # Safety
/// Must be called on the configuration-building thread with valid objects.
unsafe fn attach_common_devices(
    configuration: &VZVirtualMachineConfiguration,
    network: Retained<VZVirtioNetworkDeviceConfiguration>,
) {
    // SAFETY: plain configuration objects created and attached on this thread.
    unsafe {
        configuration.setKeyboards(&NSArray::from_retained_slice(&[Retained::into_super(
            VZUSBKeyboardConfiguration::new(),
        )]));
        configuration.setPointingDevices(&NSArray::from_retained_slice(&[Retained::into_super(
            VZUSBScreenCoordinatePointingDeviceConfiguration::new(),
        )]));
        configuration.setEntropyDevices(&NSArray::from_retained_slice(&[Retained::into_super(
            VZVirtioEntropyDeviceConfiguration::new(),
        )]));
        configuration.setNetworkDevices(&NSArray::from_retained_slice(&[Retained::into_super(
            network,
        )]));
        // Sheprd Agent answers on this, with no network between.
        configuration.setSocketDevices(&NSArray::from_retained_slice(&[Retained::into_super(
            VZVirtioSocketDeviceConfiguration::new(),
        )]));
    }
}

/// A completion handler that calls `done` once, with what went wrong.
fn once(
    app: AsyncApp,
    done: impl FnOnce(Option<String>, &mut App) + 'static,
) -> RcBlock<dyn Fn(*mut NSError)> {
    let done = Cell::new(Some(done));
    RcBlock::new(move |error: *mut NSError| {
        // SAFETY: Virtualization passes null or an error that outlives this call.
        let message = NonNull::new(error).map(|error| description(unsafe { error.as_ref() }));
        if let Some(done) = done.take() {
            app.update(|cx| done(message, cx));
        }
    })
}

/// The connection's own descriptor, which outlives it: the connection is
/// closed here, and the copy is the caller's.
fn opened_vsock(
    name: &Name,
    connection: *mut VZVirtioSocketConnection,
    error: *mut NSError,
) -> agent::Result<OwnedFd> {
    // SAFETY: Virtualization passes null or objects that outlive this call.
    let (connection, error) = unsafe { (connection.as_ref(), error.as_ref()) };
    let unreachable = |reason: String| agent::Error::Unreachable {
        name: name.clone(),
        reason,
    };
    let Some(connection) = connection else {
        return Err(unreachable(error.map_or_else(String::new, description)));
    };
    // SAFETY: the descriptor stays open until the connection closes below,
    // after it was copied.
    let copied = unsafe {
        let fd = BorrowedFd::borrow_raw(connection.fileDescriptor());
        let copied = fd.try_clone_to_owned();
        connection.close();
        copied
    };
    copied.map_err(agent::Error::Io)
}

pub(crate) fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

pub(crate) fn description(error: &NSError) -> String {
    error.localizedDescription().to_string()
}
