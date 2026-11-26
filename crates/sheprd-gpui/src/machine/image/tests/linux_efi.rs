use super::*;
use crate::machine::metadata::{GuestMetadata, GuestOs, write_in};

#[test]
fn linux_efi_image_clones_efi_vars_and_metadata_without_macos_guide() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("linux-dev");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join(DISK), b"linux-disk").unwrap();
    fs::write(source.join(EFI_VARS), b"efi-vars").unwrap();
    fs::write(source.join(IDENTITY), b"old-id").unwrap();
    fs::write(source.join(SEED_ISO), b"old-seed").unwrap();
    write_in(&source, &GuestMetadata::linux_apple_efi(None, false)).unwrap();

    let images = root.path().join("images");
    save_in(&source, &name("linux-dev"), &images, &name("ubuntu")).unwrap();
    let image = images.join("ubuntu");
    assert_eq!(fs::read(image.join(DISK)).unwrap(), b"linux-disk");
    assert_eq!(fs::read(image.join(EFI_VARS)).unwrap(), b"efi-vars");
    assert!(!image.join(IDENTITY).exists());
    assert!(!image.join(SEED_ISO).exists());
    assert!(!image.join(AUXILIARY).exists());
    assert!(!image.join(HARDWARE).exists());

    let listed = list_in(&images).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].guest.os, GuestOs::Linux);

    let machines = root.path().join("machines");
    create_in(
        &image,
        &name("ubuntu"),
        &machines,
        &name("clone-1"),
        b"new-id-1",
    )
    .unwrap();
    create_in(
        &image,
        &name("ubuntu"),
        &machines,
        &name("clone-2"),
        b"new-id-2",
    )
    .unwrap();
    let clone1 = machines.join("clone-1");
    let clone2 = machines.join("clone-2");
    assert_eq!(fs::read(clone1.join(IDENTITY)).unwrap(), b"new-id-1");
    assert_eq!(fs::read(clone2.join(IDENTITY)).unwrap(), b"new-id-2");
    assert_eq!(fs::read(clone1.join(EFI_VARS)).unwrap(), b"efi-vars");
    assert_eq!(metadata::read_in(&clone1).unwrap().os, GuestOs::Linux);
    // Linux machines do not open the macOS setup guide.
    assert!(!clone1.join(GUIDE).exists());
    // Each Linux clone receives a distinct seed.iso with unique instance-id and hostname.
    let seed1 = fs::read(clone1.join(SEED_ISO)).unwrap();
    let seed2 = fs::read(clone2.join(SEED_ISO)).unwrap();
    assert_ne!(seed1, seed2);
    let seed1_text = String::from_utf8_lossy(&seed1);
    let seed2_text = String::from_utf8_lossy(&seed2);
    assert!(seed1_text.contains(&seed::instance_id(b"new-id-1")));
    assert!(seed1_text.contains("local-hostname: clone-1"));
    assert!(seed2_text.contains(&seed::instance_id(b"new-id-2")));
    assert!(seed2_text.contains("local-hostname: clone-2"));
    assert_ne!(
        crate::machine::configuration::mac_address(b"new-id-1"),
        crate::machine::configuration::mac_address(b"new-id-2"),
    );
}
