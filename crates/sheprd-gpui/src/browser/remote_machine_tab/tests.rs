use super::{key_input, named};
use gpui::{Keystroke, Modifiers};
use sheprd_control_protocol::protocol::tabs::{MachineModifier, TabOp};

fn stroke(text: &'static str) -> Keystroke {
    Keystroke::parse(text).unwrap_or_else(|_| Keystroke {
        modifiers: Modifiers::default(),
        key: text.to_owned(),
        key_char: None,
    })
}

#[test]
fn typed_text_types_and_named_keys_press() {
    let mut typed = stroke("a");
    typed.key_char = Some("a".to_owned());
    assert!(matches!(key_input(&typed), Some(TabOp::MachineType { text, .. }) if text == "a"));
    assert!(matches!(
        key_input(&stroke("enter")),
        Some(TabOp::MachineKey { key, .. }) if key == "return"
    ));
    assert!(matches!(
        key_input(&stroke("ctrl-c")),
        Some(TabOp::MachineKey { key, modifiers, .. })
            if key == "c" && modifiers == [MachineModifier::Control]
    ));
}

#[test]
fn the_apps_own_shortcuts_stay_the_apps() {
    assert!(key_input(&stroke("cmd-w")).is_none());
}

#[test]
fn input_is_aimed_at_the_tabs_machine() {
    let op = named(
        TabOp::MachineType {
            name: String::new(),
            text: "x".to_owned(),
        },
        "default".to_owned(),
    );
    assert!(matches!(op, TabOp::MachineType { name, .. } if name == "default"));
}
