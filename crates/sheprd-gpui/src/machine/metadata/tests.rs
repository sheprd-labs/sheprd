#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn missing_metadata_defaults_to_macos_arm64() {
    let folder = tempfile::tempdir().unwrap();
    let metadata = read_in(folder.path()).unwrap();
    assert_eq!(metadata, GuestMetadata::macos_default());
    assert_eq!(metadata.os, GuestOs::Macos);
    assert_eq!(metadata.arch, GuestArch::Arm64);
    assert_eq!(metadata.capabilities(), MachineCapabilities::macos());
    assert_eq!(metadata.image_label(), None);
}

#[test]
fn linux_metadata_round_trips_and_reports_capabilities() {
    let folder = tempfile::tempdir().unwrap();
    let metadata = GuestMetadata::linux_apple_efi(
        Some(ImageIdentity {
            id: "ubuntu-wayland".into(),
            version: Some("24.04.5".into()),
            sha256: Some("abc123".into()),
        }),
        true,
    );
    write_in(folder.path(), &metadata).unwrap();
    let loaded = read_in(folder.path()).unwrap();
    assert_eq!(loaded, metadata);
    assert_eq!(
        loaded.image_label().as_deref(),
        Some("ubuntu-wayland@24.04.5")
    );
    assert_eq!(loaded.capabilities(), MachineCapabilities::linux_apple());
    assert!(!loaded.capabilities().save_restore);
    assert!(loaded.capabilities().graceful_shutdown);
    assert!(!loaded.capabilities().dynamic_display);
}

#[test]
fn damaged_or_future_metadata_is_refused_rather_than_treated_as_macos() {
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join(FILE), b"{bad json").unwrap();
    assert!(matches!(
        read_in(folder.path()),
        Err(Error::InvalidMetadata { .. })
    ));

    fs::write(
        folder.path().join(FILE),
        br#"{"version":99,"os":"linux","arch":"arm64","boot":{"kind":"apple_linux_efi"}}"#,
    )
    .unwrap();
    assert!(matches!(
        read_in(folder.path()),
        Err(Error::InvalidMetadata { .. })
    ));
}

#[test]
fn mismatched_os_and_boot_metadata_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    fs::write(
        folder.path().join(FILE),
        br#"{"version":1,"os":"linux","arch":"arm64","boot":{"kind":"apple_macos"}}"#,
    )
    .unwrap();
    assert!(matches!(
        read_in(folder.path()),
        Err(Error::InvalidMetadata { .. })
    ));
}
