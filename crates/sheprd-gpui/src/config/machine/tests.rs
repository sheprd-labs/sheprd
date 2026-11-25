use super::*;

#[test]
fn lends_by_default_and_adds_hosts_once() {
    let config = MachineConfig::default();
    assert!(config.lend);
    let hosts = config.adding("mini-2").unwrap_or_default();
    assert_eq!(hosts, ["mini-2"]);
    let config = MachineConfig {
        borrow_from: hosts,
        ..MachineConfig::default()
    };
    assert!(config.borrows_from("mini-2"));
    assert!(config.adding("mini-2").is_none());
}

#[test]
fn reads_the_table() {
    let config: MachineConfig =
        toml::from_str("lend = false\nborrow_from = [\"mini-1\"]").unwrap_or_default();
    assert!(!config.lend);
    assert!(config.borrows_from("mini-1"));
}
