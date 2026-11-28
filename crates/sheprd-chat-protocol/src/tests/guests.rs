use serde_json::json;

use crate::*;

#[test]
fn a_guest_prompt_reads_as_documented() {
    let command: ClientMessage = serde_json::from_value(json!({
        "type": "command",
        "id": "6f1c1b40-0000-4000-8000-000000000001",
        "command": {"command": "guest_prompt", "text": "hi", "guest": {"name": "Bob", "tool": "tma"}},
    }))
    .unwrap();
    let ClientMessage::Command {
        command: Command::GuestPrompt { text, guest },
        ..
    } = command
    else {
        panic!("not a guest prompt");
    };
    assert_eq!(text, "hi");
    assert_eq!(guest, Guest::new("Bob", "tma"));
    assert_eq!(guest.label(), "Bob via tma");
}

#[test]
fn guest_names_are_bounded_labels() {
    assert!(Guest::new("Bob", "tma").is_valid());
    assert!(Guest::new("Zoë Martín", "tma").is_valid());
    assert!(!Guest::new(" ", "tma").is_valid());
    assert!(!Guest::new("Bob", "").is_valid());
    assert!(!Guest::new("x".repeat(MAX_GUEST_NAME + 1), "tma").is_valid());
    assert!(!Guest::new("Bob", "x".repeat(MAX_GUEST_TOOL + 1)).is_valid());
    for bad in [
        "Bo\nb",
        "Bob\"",
        "<Bob>",
        "Bob & co",
        "`Bob`",
        "Bob\u{1b}[2J",
    ] {
        assert!(!Guest::new(bad, "tma").is_valid(), "{bad:?}");
    }
}

#[test]
fn guest_fields_are_optional_and_left_out_when_absent() {
    let user: EntryKind = serde_json::from_value(json!({"type": "user", "text": "hi"})).unwrap();
    assert!(matches!(user, EntryKind::User { guest: None, .. }));
    let value = serde_json::to_value(&user).unwrap();
    assert!(value.get("guest").is_none());

    let guest = EntryKind::User {
        guest: Some(Guest::new("Bob", "tma")),
        text: "hi".to_owned(),
        files: 0,
        note: None,
    };
    let value = serde_json::to_value(&guest).unwrap();
    assert_eq!(value["guest"], json!({"name": "Bob", "tool": "tma"}));

    let origin = Origin::guest(Guest::new("Bob", "tma"));
    assert!(!origin.opens_paths());
    let older: Origin =
        serde_json::from_value(json!({"device": "desktop", "via": "local"})).unwrap();
    assert_eq!(older.guest, None);
}

#[test]
fn a_guest_permission_carries_the_prompt_and_older_rejections_still_read() {
    let permission: Permission = serde_json::from_value(json!({
        "id": 3, "title": "Bob via tma asks to send: hi", "call_id": null,
        "options": [
            {"id": GUEST_ALLOW, "kind": "allow_once", "name": "Send to the agent"},
            {"id": GUEST_DENY, "kind": "reject_once", "name": "Decline"}],
        "guest": {"guest": {"name": "Bob", "tool": "tma"}, "text": "hi"},
    }))
    .unwrap();
    assert_eq!(permission.guest.unwrap().text, "hi");
    for (rejection, name) in [
        (Rejection::Denied, "denied"),
        (Rejection::GuestOnly, "guest_only"),
        (Rejection::InvalidGuest, "invalid_guest"),
    ] {
        assert_eq!(serde_json::to_value(rejection).unwrap(), json!(name));
    }
    let newer: Rejection = serde_json::from_value(json!("guest_expired")).unwrap();
    assert_eq!(newer, Rejection::Other);
}

#[test]
fn guest_text_must_show_as_written() {
    assert!(shows_as_written(
        "Line one\n\tLine two, café, 日本語, 👍🏽 ❤\u{fe0f} 👨\u{200d}👩\u{200d}👧"
    ));
    for hidden in [
        "a\u{200b}b",
        "\u{202e}abc",
        "x\u{2066}y",
        "\u{feff}x",
        "a\rb",
        "\u{e0061}",
    ] {
        assert!(!shows_as_written(hidden), "{hidden:?}");
    }
    assert_eq!(
        serde_json::to_value(Rejection::HiddenText).unwrap(),
        json!("hidden_text")
    );
}
