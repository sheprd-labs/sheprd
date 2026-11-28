use super::*;

#[test]
fn auto_accept_defaults_off_and_can_be_opted_in() {
    assert!(!ChatConfig::default().auto_accept);
    assert!(!crate::config::Config::parse("").unwrap().chat.auto_accept);
    assert!(
        crate::config::Config::parse("[chat]\nauto_accept = true\n")
            .unwrap()
            .chat
            .auto_accept
    );
    assert!(
        !crate::config::Config::parse("[chat]\nauto_accept = false\n")
            .unwrap()
            .chat
            .auto_accept
    );
}

#[test]
fn auto_accept_is_a_recognized_strict_boolean() {
    for enabled in [false, true] {
        let config =
            crate::config::Config::parse(&format!("[chat]\nauto_accept = {enabled}\n")).unwrap();
        assert!(config.unknown_keys.is_empty());
    }
    for value in ["1", "'true'", "[]"] {
        assert!(crate::config::Config::parse(&format!("[chat]\nauto_accept = {value}\n")).is_err());
    }
}
