use super::*;
use crate::config::preferences::Preference;

#[test]
fn new_terminals_stay_herdr_1_until_chosen() -> anyhow::Result<()> {
    assert!(!Config::default().herdr_2_terminals);
    assert!(!Config::parse("")?.herdr_2_terminals);
    assert!(!Config::parse(DEFAULT_CONFIG)?.herdr_2_terminals);
    assert!(Config::parse("herdr_2_terminals = true")?.herdr_2_terminals);
    assert!(Config::parse("herdr_2_terminals = 'yes'").is_err());
    Ok(())
}

#[test]
fn choosing_herdr_2_saves_in_place() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    fs::write(&path, "# personal\nlayout = 'orca' # mode\n")?;
    Config::save_preference_path(Preference::TerminalBackend(TerminalBackend::Core), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# personal") && text.contains("# mode"));
    assert!(Config::parse(&text)?.herdr_2_terminals);
    Ok(())
}

#[test]
fn explicit_backend_overrides_legacy_preference() -> anyhow::Result<()> {
    for old in [false, true] {
        let config = Config::parse(&format!(
            "herdr_2_terminals = {old}\nterminal_backend = 'tmux'"
        ))?;
        let expected = if cfg!(feature = "tmux") {
            TerminalBackend::Tmux
        } else if old {
            TerminalBackend::Core
        } else {
            TerminalBackend::Herdr
        };
        assert_eq!(config.terminal_backend, expected);
    }
    assert!(Config::parse("terminal_backend = 'other'").is_err());
    Ok(())
}

#[test]
fn saved_tmux_panes_keep_their_owner_in_both_builds() -> anyhow::Result<()> {
    let id = sheprd_backend::TerminalId::new("tmux-01234567-89ab-cdef-0123-456789abcdef")?;
    let owner = TerminalBackend::owner(&id);
    assert_eq!(owner, TerminalBackend::Tmux);
    assert_eq!(
        owner.available(cfg!(feature = "tmux")),
        cfg!(feature = "tmux")
    );
    Ok(())
}
