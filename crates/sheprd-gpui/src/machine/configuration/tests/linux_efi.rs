use super::*;
use crate::machine::metadata::{GuestMetadata, GuestOs, MachineCapabilities, write_in};

#[test]
fn linux_efi_bundle_reads_efi_vars_ignores_state_and_skips_macos_guide() {
    let root = tempfile::tempdir().expect("tempdir");
    let folder = root.path().join("linux-dev");
    fs::create_dir_all(&folder).expect("dir");
    fs::write(folder.join(DISK), b"raw-disk").expect("disk");
    fs::write(folder.join(EFI_VARS), b"efi-vars").expect("efi");
    fs::write(folder.join(IDENTITY), b"generic-id").expect("id");
    // Even if stale state or macOS guide files exist, Linux ignores them.
    fs::write(folder.join(store::STATE), b"stale").expect("state");
    fs::write(folder.join(GUIDE), b"").expect("guide");
    write_in(&folder, &GuestMetadata::linux_apple_efi(None, true)).expect("metadata");

    let bundle = Bundle::read_in(&folder, true).expect("bundle");
    assert_eq!(bundle.guest.os, GuestOs::Linux);
    assert_eq!(bundle.capabilities(), MachineCapabilities::linux_apple());
    assert_eq!(bundle.saved, None);
    assert_eq!(bundle.guide, None);
    assert!(matches!(
        bundle.boot,
        BootFiles::LinuxEfi {
            seed_iso: None,
            serial_log: Some(_),
            ..
        }
    ));

    // When `seed.iso` is present, it is attached read-only.
    fs::write(folder.join(SEED_ISO), b"cidata").expect("seed");
    let with_seed = Bundle::read_in(&folder, true).expect("bundle with seed");
    assert!(matches!(
        with_seed.boot,
        BootFiles::LinuxEfi {
            seed_iso: Some(_),
            serial_log: Some(_),
            ..
        }
    ));
}

#[test]
fn linux_efi_bundle_requires_efi_variable_store() {
    let root = tempfile::tempdir().expect("tempdir");
    let folder = root.path().join("linux-dev");
    fs::create_dir_all(&folder).expect("dir");
    fs::write(folder.join(DISK), b"raw-disk").expect("disk");
    fs::write(folder.join(IDENTITY), b"generic-id").expect("id");
    write_in(&folder, &GuestMetadata::linux_apple_efi(None, false)).expect("metadata");

    assert!(matches!(
        Bundle::read_in(&folder, false),
        Err(Error::NotInstalled(_))
    ));
}
