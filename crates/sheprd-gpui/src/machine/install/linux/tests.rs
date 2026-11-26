#![allow(clippy::unwrap_used)]
use super::*;
use crate::machine::{
    configuration::{BootFiles, Bundle},
    metadata::GuestOs,
};

fn name(text: &str) -> Name {
    Name::try_from(text.to_owned()).unwrap()
}

/// Minimal valid QCOW2 v3 image (4 KiB virtual size, one uncompressed cluster).
fn tiny_qcow2(marker: &[u8]) -> Vec<u8> {
    let cluster = 4096usize;
    let mut img = vec![0u8; cluster * 4];
    img[0..4].copy_from_slice(b"QFI\xfb");
    img[4..8].copy_from_slice(&3u32.to_be_bytes());
    img[20..24].copy_from_slice(&12u32.to_be_bytes());
    img[24..32].copy_from_slice(&(cluster as u64).to_be_bytes());
    img[36..40].copy_from_slice(&1u32.to_be_bytes());
    img[40..48].copy_from_slice(&(cluster as u64).to_be_bytes()); // l1
    img[96..100].copy_from_slice(&4u32.to_be_bytes()); // refcount_order
    img[100..104].copy_from_slice(&104u32.to_be_bytes()); // header_length
    img[cluster..cluster + 8].copy_from_slice(&((cluster * 2) as u64).to_be_bytes()); // l2
    img[cluster * 2..cluster * 2 + 8].copy_from_slice(&((cluster * 3) as u64).to_be_bytes());
    img[cluster * 3..cluster * 3 + marker.len()].copy_from_slice(marker);
    img
}

#[test]
fn catalog_finds_pinned_ubuntu_wayland_and_rejects_unknown() {
    let entry = find(DEFAULT_IMAGE_ID).unwrap();
    assert_eq!(entry.id, "ubuntu-wayland");
    assert!(entry.url.starts_with("https://"));
    assert_eq!(entry.sha256.len(), 64);
    assert!(matches!(find("nonexistent"), Err(Error::UnknownImage(_))));
}

#[test]
fn linux_first_settings_respect_host_limits_and_use_fixed_display() {
    let roomy = Limits::of(12, 32 << 30);
    let settings = first_settings(&UBUNTU_WAYLAND, &roomy);
    assert_eq!(settings.cpus, 4);
    assert_eq!(settings.memory_gib, 4);
    assert_eq!(
        settings.display,
        Display::Fixed {
            width: LINUX_DEFAULT_WIDTH,
            height: LINUX_DEFAULT_HEIGHT
        }
    );

    let small = Limits::of(2, 8 << 30);
    let settings = first_settings(&UBUNTU_WAYLAND, &small);
    assert_eq!(settings.cpus, 2);
    assert_eq!(settings.memory_gib, 4);

    let tiny = Limits {
        cpus: 1..=2,
        memory_gib: 1..=2,
    };
    let settings = first_settings(&UBUNTU_WAYLAND, &tiny);
    assert_eq!(settings.cpus, 2);
    assert_eq!(settings.memory_gib, 2);
}

#[test]
fn staging_stays_invisible_until_published_and_bundle_reads_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("machines");
    let qcow2_path = dir.path().join("ubuntu.img");
    fs::write(&qcow2_path, tiny_qcow2(b"UBUNTU-ROOT")).unwrap();

    let mut test_entry = UBUNTU_WAYLAND;
    test_entry.disk_bytes = 64 * 1024;

    let machine_name = name("linux-dev");
    let limits = Limits::of(8, 16 << 30);
    let staging = prepare_in(
        &StageRequest {
            root: &root,
            name: &machine_name,
            entry: &test_entry,
            qcow2_path: &qcow2_path,
            identity: b"generic-id-1",
            limits: &limits,
        },
        |efi_path| {
            fs::write(efi_path, b"efi-store").map_err(|source| store::write_error(efi_path, source))
        },
        &AtomicBool::new(false),
    )
    .unwrap();

    // While staged under `.install-*`, `store::list_in` must NOT see it.
    assert!(store::list_in_path(&root).unwrap().is_empty());

    let published = publish_in(staging, &root, &name("linux-dev")).unwrap();
    let listed = store::list_in_path(&root).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, name("linux-dev"));
    assert_eq!(listed[0].guest.os, GuestOs::Linux);
    assert!(!listed[0].saved);
    assert!(!listed[0].ready);
    assert_eq!(
        listed[0].guest.image_label().as_deref(),
        Some("ubuntu-wayland@24.04.5-20261017")
    );

    let bundle = Bundle::read_in(&published, true).unwrap();
    assert_eq!(bundle.guest.os, GuestOs::Linux);
    assert!(matches!(
        bundle.boot,
        BootFiles::LinuxEfi {
            seed_iso: Some(_),
            serial_log: None,
            ..
        }
    ));
}

#[test]
fn cancelled_staging_cleans_up_and_leaves_no_machine() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("machines");
    let qcow2_path = dir.path().join("ubuntu.img");
    fs::write(&qcow2_path, tiny_qcow2(b"UBUNTU-ROOT")).unwrap();

    let cancel = AtomicBool::new(true);
    let machine_name = name("linux-dev");
    let limits = Limits::of(8, 16 << 30);
    let result = prepare_in(
        &StageRequest {
            root: &root,
            name: &machine_name,
            entry: &UBUNTU_WAYLAND,
            qcow2_path: &qcow2_path,
            identity: b"generic-id-2",
            limits: &limits,
        },
        |_| Ok(()),
        &cancel,
    );
    assert!(matches!(
        result,
        Err(crate::machine::Error::Install(Error::Cancelled))
    ));
    assert!(!root.join("linux-dev").exists());
}
