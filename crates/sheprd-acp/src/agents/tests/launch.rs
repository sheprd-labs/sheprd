use super::*;

#[test]
fn claude_itself_is_used_unless_the_adapter_is_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "claude-agent-acp");
    let path = dir.path().as_os_str();
    let adapter = Launch::Acp(
        dir.path().join("claude-agent-acp").into_os_string(),
        Vec::new(),
    );
    assert_eq!(
        launch(Agent::Claude, Some(path), false),
        adapter,
        "no claude yet"
    );
    install(dir.path(), "claude");
    assert_eq!(
        launch(Agent::Claude, Some(path), false),
        Launch::Native(dir.path().join("claude"))
    );
    assert_eq!(launch(Agent::Claude, Some(path), true), adapter);
}

#[test]
fn codex_and_pi_are_reached_directly_too() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "codex");
    install(dir.path(), "pi");
    let path = dir.path().as_os_str();
    assert_eq!(
        launch(Agent::Codex, Some(path), false),
        Launch::Native(dir.path().join("codex"))
    );
    assert_eq!(
        launch(Agent::Pi, Some(path), false),
        Launch::Native(dir.path().join("pi"))
    );
    assert_eq!(installed(Some(path)), [Agent::Codex, Agent::Pi]);
    assert!(matches!(
        launch(Agent::Pi, Some(path), true),
        Launch::Acp(..)
    ));
}

#[test]
fn other_agents_keep_their_acp_command() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "claude");
    install(dir.path(), "opencode");
    let path = dir.path().as_os_str();
    assert_eq!(
        launch(Agent::Opencode, Some(path), false),
        Launch::Acp(
            dir.path().join("opencode").into_os_string(),
            vec!["acp".into()]
        )
    );
}

#[test]
fn muse_runs_its_own_acp_server() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "muse");
    assert_eq!(
        launch(Agent::Muse, Some(dir.path().as_os_str()), false),
        Launch::Acp(dir.path().join("muse").into_os_string(), vec!["acp".into()])
    );
}

#[test]
fn antigravity_runs_its_official_acp_command() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "agy");
    let path = dir.path().as_os_str();
    assert_eq!(
        launch(Agent::Antigravity, Some(path), false),
        Launch::Acp(dir.path().join("agy").into_os_string(), vec!["acp".into()])
    );
}

#[cfg(unix)]
#[test]
fn the_standalone_antigravity_cli_beats_an_ide_launcher_on_path() {
    let ide = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    install(ide.path(), "agy");
    let local_bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&local_bin).unwrap();
    install(&local_bin, "agy");
    let path = search_path(Some(ide.path().as_os_str()), Some(home.path()));

    assert_eq!(
        antigravity_program(Some(path.as_os_str()), Some(home.path())),
        Some(local_bin.join("agy"))
    );
}

#[test]
fn claude_alone_counts_as_installed() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "claude");
    assert_eq!(installed(Some(dir.path().as_os_str())), [Agent::Claude]);
}
