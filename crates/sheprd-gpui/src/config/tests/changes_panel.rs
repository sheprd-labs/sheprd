use super::*;

#[test]
fn changes_panel_is_off_until_the_config_names_it() -> anyhow::Result<()> {
    assert!(!Config::default().show_changes_panel);
    assert!(!Config::parse("")?.show_changes_panel);
    assert!(Config::parse("show_changes_panel = true")?.show_changes_panel);
    Ok(())
}
