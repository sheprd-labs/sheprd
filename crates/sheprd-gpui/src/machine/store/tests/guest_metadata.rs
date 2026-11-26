use super::*;
use crate::machine::metadata::{GuestMetadata, GuestOs, ImageIdentity, write_in};

#[test]
fn legacy_folder_without_metadata_lists_as_macos_with_save_restore() {
    let root = tempfile::tempdir().unwrap();
    installed(root.path(), "legacy");
    let listed = list_in(root.path()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].guest.os, GuestOs::Macos);
    assert!(listed[0].capabilities().save_restore);
}

#[test]
fn linux_folder_never_reports_suspended_even_with_stale_state_file() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "linux-dev");
    write_in(
        &folder,
        &GuestMetadata::linux_apple_efi(
            Some(ImageIdentity {
                id: "ubuntu-wayland".into(),
                version: Some("24.04".into()),
                sha256: None,
            }),
            false,
        ),
    )
    .unwrap();
    fs::write(folder.join(STATE), b"stale").unwrap();

    let listed = list_in(root.path()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].guest.os, GuestOs::Linux);
    assert_eq!(
        listed[0].guest.image_label().as_deref(),
        Some("ubuntu-wayland@24.04")
    );
    assert!(!listed[0].saved);
    assert!(!listed[0].capabilities().save_restore);

    let chosen = Settings {
        cpus: 6,
        ..Settings::default()
    };
    write_json(&folder.join(SETTINGS), &chosen).unwrap();
    assert_eq!(boot_settings(&folder, false).unwrap(), chosen);
}

#[test]
fn damaged_metadata_folder_is_skipped_rather_than_listed_as_macos() {
    let root = tempfile::tempdir().unwrap();
    let broken = installed(root.path(), "broken");
    fs::write(broken.join(GUEST_METADATA), b"{not valid json").unwrap();
    installed(root.path(), "good");

    let listed = list_in(root.path()).unwrap();
    let names: Vec<_> = listed.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["good"]);
}
