use super::*;

#[cfg(unix)]
#[test]
fn only_agents_with_their_own_command_are_installed() {
    let dir = tempfile::tempdir().unwrap();
    // npx alone would only download Claude's and Codex's adapters.
    install(dir.path(), "npx");
    install(dir.path(), "agy");
    install(dir.path(), "muse");
    install(dir.path(), "opencode");
    install(dir.path(), "codex-acp");
    let path = dir.path().as_os_str();
    assert_eq!(
        installed(Some(path)),
        [
            Agent::Antigravity,
            Agent::Codex,
            Agent::Muse,
            Agent::Opencode
        ]
    );
    assert!(installed(None).is_empty());
}

#[test]
fn the_search_path_adds_user_and_system_folders_after_path() {
    let home = Path::new("/home/me");
    let path = search_path(Some(OsStr::new("/usr/bin")), Some(home));
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    assert_eq!(dirs[0], PathBuf::from("/usr/bin"));
    assert!(dirs.contains(&home.join(".local/bin")));
    assert!(dirs.contains(&home.join(".opencode/bin")));
    let without_home = search_path(None, None);
    assert!(std::env::split_paths(&without_home).all(|dir| !dir.starts_with("/home/me")));
}

#[cfg(windows)]
#[test]
fn windows_search_path_adds_and_prefers_standalone_antigravity_dir() {
    let ide = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let agy_bin = local.path().join("agy").join("bin");
    std::fs::create_dir_all(&agy_bin).unwrap();
    std::fs::write(ide.path().join("agy.cmd"), "@echo off\r\n").unwrap();
    std::fs::write(agy_bin.join("agy.exe"), "").unwrap();

    let path = search_path_with(Some(ide.path().as_os_str()), None, Some(local.path()));
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    assert!(dirs.contains(&agy_bin));
    assert_eq!(
        antigravity_program_with(Some(&path), None, Some(local.path())),
        Some(agy_bin.join("agy.exe"))
    );

    // Without LOCALAPPDATA, falls back to %USERPROFILE%\AppData\Local\agy\bin.
    let home = tempfile::tempdir().unwrap();
    let fallback = home
        .path()
        .join("AppData")
        .join("Local")
        .join("agy")
        .join("bin");
    let path = search_path_with(None, Some(home.path()), None);
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    assert!(dirs.contains(&fallback));
}

#[cfg(windows)]
#[test]
fn windows_finds_the_muse_installer_shim_outside_path() {
    let local = tempfile::tempdir().unwrap();
    let bin = local.path().join("Programs").join("muse");
    std::fs::create_dir_all(&bin).unwrap();
    let shim = bin.join("muse.cmd");
    std::fs::write(&shim, "@echo off\r\n").unwrap();
    let path = search_path_with(None, None, Some(local.path()));
    assert_eq!(
        launch(Agent::Muse, Some(&path), false),
        Launch::Acp(shim.into_os_string(), vec!["acp".into()])
    );
}
