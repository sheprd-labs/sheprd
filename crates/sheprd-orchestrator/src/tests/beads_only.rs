use super::*;
use crate::{repo, service::sources};
use sheprd_client::ConnectTarget;
use std::sync::atomic::AtomicBool;

#[test]
fn a_beads_folder_outside_git_is_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = dir.path().join("notes");
    std::fs::create_dir_all(checkout.join(".beads")).unwrap();
    std::fs::write(checkout.join(".beads/metadata.json"), "{}").unwrap();
    let host = sheprd_remote::teleport::Host::new(&ConnectTarget::Local).unwrap();
    let info = repo::detect(
        &ConnectTarget::Local,
        &host,
        &checkout.to_string_lossy(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let root = repo::plain(checkout.canonicalize().unwrap());
    assert_eq!(info.repository, Repository::Local(root.join(".beads")));
    assert_eq!(info.main_root, root.to_string_lossy());
    assert!(info.remote.is_none());
    let source = info.beads_source().unwrap();
    assert_eq!(source.provider, Provider::Beads);
    assert_eq!(source.repository, info.main_root);
    assert_eq!(sources(&info, None), vec![(source, true)]);
}

#[test]
fn ssh_beads_only_projects_have_no_git_source() {
    let target = ConnectTarget::Ssh {
        target: "devbox".into(),
        session: "default".into(),
    };
    let info = repo::parse(
        &target,
        "common\t/projects/notes/.beads\nmain\t/projects/notes\nbeads\t1\n",
    )
    .unwrap();
    assert_eq!(
        info.repository,
        Repository::Ssh {
            destination: "devbox".into(),
            git_dir: "/projects/notes/.beads".into(),
        }
    );
    assert!(info.remote.is_none());
    assert_eq!(
        sources(&info, None),
        vec![(info.beads_source().unwrap(), true)]
    );
}

#[test]
fn a_file_named_beads_does_not_make_a_project() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".beads"), "").unwrap();
    let host = sheprd_remote::teleport::Host::new(&ConnectTarget::Local).unwrap();
    assert!(matches!(
        repo::detect(
            &ConnectTarget::Local,
            &host,
            &dir.path().to_string_lossy(),
            &AtomicBool::new(false),
        ),
        Err(Error::NotARepository)
    ));
}

/// Whether `checkout` is a project, read on this machine and, where a shell
/// runs it, by the SSH script, which must agree.
fn found(checkout: &std::path::Path) -> bool {
    let local = beads(detect_local(checkout));
    #[cfg(unix)]
    assert_eq!(beads(script(checkout)), local, "SSH script disagrees");
    local
}

fn detect_local(checkout: &std::path::Path) -> Result<repo::RepoInfo> {
    let host = sheprd_remote::teleport::Host::new(&ConnectTarget::Local).unwrap();
    repo::detect(
        &ConnectTarget::Local,
        &host,
        &checkout.to_string_lossy(),
        &AtomicBool::new(false),
    )
}

/// What the SSH discovery script reads of `checkout` under a local shell.
#[cfg(unix)]
fn script(checkout: &std::path::Path) -> Result<repo::RepoInfo> {
    let mut command = std::process::Command::new("/bin/sh");
    command
        .args(["-c", &repo::detection_script(&checkout.to_string_lossy())])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", checkout)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", checkout.join("gitconfig"));
    let output = sheprd_remote::teleport::Host::capture_command(
        command,
        std::time::Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .map_err(|_| Error::Output {
        operation: "Running the discovery script",
    })?;
    repo::parse(
        &ConnectTarget::Ssh {
            target: "fixture".into(),
            session: "default".into(),
        },
        &String::from_utf8(output).unwrap(),
    )
}

fn beads(info: Result<repo::RepoInfo>) -> bool {
    match info {
        Ok(info) => info.beads,
        Err(Error::NotARepository) => false,
        Err(error) => panic!("{error:?}"),
    }
}

#[test]
fn bds_own_state_beside_a_home_folder_is_not_a_project() {
    for git in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        if git {
            gix::init(dir.path()).unwrap();
        }
        let state = dir.path().join(".beads");
        std::fs::create_dir_all(state.join("eventsData")).unwrap();
        std::fs::write(state.join("machine-id"), "0123").unwrap();
        std::fs::write(state.join("vc.db"), "").unwrap();
        std::fs::write(state.join("issues.db.backup"), "").unwrap();
        assert!(!found(dir.path()), "git: {git}");
    }
}

#[test]
fn what_bd_reads_as_a_project_makes_one() {
    for (name, folder) in [
        ("redirect", false),
        ("metadata.json", false),
        ("config.yaml", false),
        ("beads.db", false),
        ("dolt", true),
        ("embeddeddolt", true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join(".beads");
        std::fs::create_dir(&state).unwrap();
        if folder {
            std::fs::create_dir(state.join(name)).unwrap();
        } else {
            std::fs::write(state.join(name), "").unwrap();
        }
        assert!(found(dir.path()), "{name}");
    }
}

/// A `.beads` folder that cannot be listed fails discovery, on this machine
/// and over SSH, rather than dropping the project as if it held none.
#[cfg(unix)]
#[test]
fn an_unreadable_beads_folder_is_a_discovery_failure() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join(".beads");
    std::fs::create_dir(&state).unwrap();
    std::fs::write(state.join("beads.db"), "").unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o100)).unwrap();
    // Root reads any folder, so there is nothing to refuse.
    let readable = std::fs::read_dir(&state).is_ok();
    let local = detect_local(dir.path());
    let ssh = script(dir.path());
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    if readable {
        return;
    }
    assert!(matches!(local, Err(Error::Git { .. })), "{local:?}");
    assert!(matches!(ssh, Err(Error::Output { .. })), "{ssh:?}");
}
