use super::*;

fn folders() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("temporary folder");
    let machine = root.path().join("machine");
    fs::create_dir(&machine).expect("machine folder");
    let bundled = root.path().join("MachineAgent.img");
    fs::write(&bundled, b"new").expect("bundled disk");
    (root, machine, bundled)
}

#[test]
fn a_fresh_boot_takes_the_apps_current_installer() {
    let (_root, machine, bundled) = folders();
    fs::write(machine.join(INSTALLER), b"old").expect("old copy");
    let disk = installer(&machine, Some(&bundled), false).expect("a disk");
    assert_eq!(fs::read(disk).expect("copy"), b"new");
}

#[test]
fn a_resume_keeps_the_disk_the_guest_saw() {
    // The guest may have it mounted: changed blocks under it would read as
    // a damaged volume.
    let (_root, machine, bundled) = folders();
    fs::write(machine.join(INSTALLER), b"old").expect("old copy");
    let disk = installer(&machine, Some(&bundled), true).expect("a disk");
    assert_eq!(fs::read(disk).expect("copy"), b"old");
}

#[test]
fn a_build_outside_a_bundle_boots_without_one() {
    let (_root, machine, _bundled) = folders();
    assert_eq!(installer(&machine, None, false), None);
}

#[test]
fn a_resume_saved_without_a_disk_has_none() {
    let (_root, machine, bundled) = folders();
    assert_eq!(installer(&machine, Some(&bundled), true), None);
}
