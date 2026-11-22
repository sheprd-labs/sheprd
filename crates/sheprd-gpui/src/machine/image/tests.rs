#![allow(clippy::unwrap_used)]
use super::*;

fn name(text: &str) -> Name {
    Name::try_from(text.to_owned()).unwrap()
}

/// A stopped machine's folder under `root`.
fn machine(root: &Path, text: &str) -> PathBuf {
    let folder = root.join(text);
    fs::create_dir_all(&folder).unwrap();
    for file in [AUXILIARY, DISK, HARDWARE, IDENTITY] {
        fs::write(folder.join(file), file).unwrap();
    }
    fs::write(folder.join(SETTINGS), "{}").unwrap();
    folder
}

#[test]
fn no_folder_lists_no_images() {
    let root = tempfile::tempdir().unwrap();
    assert!(list_in(&root.path().join("none")).unwrap().is_empty());
}

#[test]
fn an_image_keeps_the_machine_but_not_who_it_was() {
    let root = tempfile::tempdir().unwrap();
    let source = machine(root.path(), "work");
    fs::write(source.join("booted.json"), "{}").unwrap();
    let images = root.path().join("images");
    save_in(&source, &name("work"), &images, &name("base")).unwrap();
    let image = images.join("base");
    assert_eq!(fs::read(image.join(DISK)).unwrap(), DISK.as_bytes());
    assert_eq!(
        fs::read(image.join(AUXILIARY)).unwrap(),
        AUXILIARY.as_bytes()
    );
    assert!(image.join(SETTINGS).is_file());
    assert!(!image.join(IDENTITY).exists());
    assert!(!image.join("booted.json").exists());
    let listed = list_in(&images).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, name("base"));
}

#[test]
fn a_suspended_machine_makes_no_image() {
    let root = tempfile::tempdir().unwrap();
    let source = machine(root.path(), "work");
    fs::write(source.join(STATE), "memory").unwrap();
    let images = root.path().join("images");
    let error = save_in(&source, &name("work"), &images, &name("base")).unwrap_err();
    assert!(matches!(error, Error::Suspended(machine) if machine == name("work")));
    assert!(!images.join("base").exists());
}

#[test]
fn a_missing_machine_makes_no_image() {
    let root = tempfile::tempdir().unwrap();
    let error = save_in(
        &root.path().join("none"),
        &name("none"),
        &root.path().join("images"),
        &name("base"),
    )
    .unwrap_err();
    assert!(matches!(error, Error::NotInstalled(_)));
}

#[test]
fn an_image_is_not_replaced() {
    let root = tempfile::tempdir().unwrap();
    let source = machine(root.path(), "work");
    let images = root.path().join("images");
    save_in(&source, &name("work"), &images, &name("base")).unwrap();
    fs::write(source.join(DISK), "changed").unwrap();
    let error = save_in(&source, &name("work"), &images, &name("base")).unwrap_err();
    assert!(matches!(error, Error::Exists(image) if image == name("base")));
    assert_eq!(
        fs::read(images.join("base").join(DISK)).unwrap(),
        DISK.as_bytes()
    );
}

#[test]
fn a_machine_from_an_image_has_its_own_identity_and_no_state() {
    let root = tempfile::tempdir().unwrap();
    let source = machine(root.path(), "work");
    let images = root.path().join("images");
    save_in(&source, &name("work"), &images, &name("base")).unwrap();
    let machines = root.path().join("machines");
    let image = images.join("base");
    create_in(&image, &name("base"), &machines, &name("one"), b"new").unwrap();
    let made = machines.join("one");
    assert_eq!(fs::read(made.join(IDENTITY)).unwrap(), b"new");
    assert_eq!(fs::read(made.join(DISK)).unwrap(), DISK.as_bytes());
    assert!(!made.join(STATE).exists());
    fs::write(made.join(DISK), "changed").unwrap();
    assert_eq!(fs::read(image.join(DISK)).unwrap(), DISK.as_bytes());
}

#[test]
fn a_missing_image_makes_no_machine() {
    let root = tempfile::tempdir().unwrap();
    let machines = root.path().join("machines");
    let error = create_in(
        &root.path().join("none"),
        &name("none"),
        &machines,
        &name("one"),
        b"new",
    )
    .unwrap_err();
    assert!(matches!(error, Error::ImageNotFound(image) if image == name("none")));
    assert!(!machines.exists());
}

#[test]
fn a_failed_clone_leaves_nothing_behind() {
    let root = tempfile::tempdir().unwrap();
    let source = machine(root.path(), "work");
    fs::remove_file(source.join(HARDWARE)).unwrap();
    let images = root.path().join("images");
    let error = save_in(&source, &name("work"), &images, &name("base")).unwrap_err();
    assert!(matches!(error, Error::Read { .. }));
    assert_eq!(fs::read_dir(&images).unwrap().count(), 0);
}

#[test]
fn settings_are_optional() {
    let root = tempfile::tempdir().unwrap();
    let source = machine(root.path(), "work");
    fs::remove_file(source.join(SETTINGS)).unwrap();
    let images = root.path().join("images");
    save_in(&source, &name("work"), &images, &name("base")).unwrap();
    assert!(!images.join("base").join(SETTINGS).exists());
}

#[test]
fn unused_counts_past_what_is_taken() {
    let taken = |text: &str| ["base", "base-2"].contains(&text);
    assert_eq!(unused(&name("free"), taken), Some(name("free")));
    assert_eq!(unused(&name("base"), taken), Some(name("base-3")));
}

#[test]
fn unused_stays_a_valid_name() {
    let long = name(&"a".repeat(crate::machine::MAX_NAME_BYTES));
    let found = unused(&long, |text| text == long.as_str()).unwrap();
    assert!(found.as_str().len() <= crate::machine::MAX_NAME_BYTES);
    assert_ne!(found, long);
}
