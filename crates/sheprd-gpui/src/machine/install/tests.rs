#![allow(clippy::unwrap_used)]
use super::*;

fn release(min_cpus: u16, min_memory_gib: u64) -> Release {
    Release {
        url: "https://updates.cdn-apple.com/macos.ipsw".to_owned(),
        build: "25A100".to_owned(),
        version: "26.0".to_owned(),
        hardware: Vec::new(),
        min_cpus,
        min_memory_bytes: min_memory_gib << 30,
    }
}

/// A new machine takes the defaults where this Mac allows them, and what
/// macOS needs where that is more.
#[test]
fn a_new_machine_starts_within_the_host_and_above_what_macos_needs() {
    let roomy = Limits::of(12, 32 << 30);
    assert_eq!(first_settings(&release(2, 4), &roomy), Settings::default());
    let small = Limits::of(4, 8 << 30);
    let settings = first_settings(&release(2, 4), &small);
    assert_eq!((settings.cpus, settings.memory_gib), (4, 4));
    let demanding = first_settings(&release(6, 12), &roomy);
    assert_eq!((demanding.cpus, demanding.memory_gib), (6, 12));
}

#[test]
fn versions_read_as_apple_writes_them() {
    assert_eq!(apple::version(26, 1, 0), "26.1");
    assert_eq!(apple::version(15, 6, 1), "15.6.1");
}

/// Another build's download, whole or not, goes once a newer one starts.
#[test]
fn starting_a_download_forgets_other_builds() {
    let folder = tempfile::tempdir().unwrap();
    let image = folder.path().join("25B10.ipsw");
    let part = download::partial(&image);
    let old = folder.path().join("25A100.ipsw");
    let old_part = download::partial(&old);
    for path in [&part, &old, &old_part] {
        fs::write(path, b"x").unwrap();
    }
    forget_other_downloads(&image);
    assert!(part.exists());
    assert!(!old.exists() && !old_part.exists());
}

/// Only an install's own staging folder goes, and only once nothing has
/// written to it for a while: another app may be installing into its own.
#[test]
fn abandoned_staging_folders_are_deleted() {
    let root = tempfile::tempdir().unwrap();
    let staged = |name: &str, age: Duration| {
        let folder = root.path().join(name);
        fs::create_dir(&folder).unwrap();
        let disk = fs::File::create(folder.join(DISK)).unwrap();
        disk.set_modified(SystemTime::now() - age).unwrap();
        folder
    };
    let abandoned = staged(".install-old", ABANDONED * 2);
    let busy = staged(".install-busy", Duration::ZERO);
    let machine = staged("work", ABANDONED * 2);
    forget_abandoned(root.path());
    assert!(!abandoned.exists());
    assert!(busy.exists());
    assert!(machine.exists());
}

/// Nothing is under way until an install begins.
#[test]
fn no_install_runs_at_first() {
    assert_eq!(progress(), None);
    let name = Name::try_from("default".to_owned()).unwrap();
    assert!(!making(&name));
}
