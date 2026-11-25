use crate::context::{Context, Facts, Prompts, Told, split};
use sheprd_chat_protocol::{Device, Origin, Via};

fn phone(id: &str) -> Origin {
    let mut origin = Origin::new(Device::PhoneApp, Via::Relay);
    origin.phone = Some(id.into());
    origin
}

#[test]
fn each_turn_targets_its_phone_and_switching_does_not_repeat_static_instructions() {
    let mut context = Context::new(
        Prompts::with_instructions("Be brief."),
        Facts::default(),
        Told::default(),
    );
    let first = context
        .before_prompt(Some(phone("phone-a")), "one")
        .unwrap();
    assert!(first.contains("Be brief."));
    assert!(first.contains("--device phone-a"));
    let again = context
        .before_prompt(Some(phone("phone-a")), "two")
        .unwrap();
    assert!(again.contains("--device phone-a"));
    assert!(!again.contains("Be brief."));
    assert!(!again.contains("Their screen is small"));
    let next = context
        .before_prompt(Some(phone("phone-b")), "three")
        .unwrap();
    assert!(next.contains("--device phone-b"));
    assert!(!next.contains("phone-a"));
    assert!(!next.contains("Be brief."));
    // Each note is kept on its own turn, including after restoring history.
    assert_eq!(split(&first).unwrap().0, first);
    let mut restored = Context::new(
        Prompts::with_instructions("Be brief."),
        Facts::default(),
        Told::default(),
    );
    restored.heard(&first);
    restored.heard(&next);
    let returned = restored
        .before_prompt(Some(phone("phone-a")), "four")
        .unwrap();
    assert!(returned.contains("--device phone-a"));
    assert!(!returned.contains("phone-b"));
    assert!(!returned.contains("Be brief."));
}

#[test]
fn desktop_and_legacy_clients_do_not_inherit_the_previous_phone() {
    for origin in [
        None,
        Some(Origin::new(Device::Desktop, Via::Local)),
        Some(Origin::new(Device::PhoneApp, Via::Lan)),
    ] {
        let mut context = Context::default();
        context.before_prompt(Some(phone("phone-a")), "one");
        let note = context.before_prompt(origin, "two").unwrap();
        assert!(note.contains("No phone target is identified"));
        assert!(!note.contains("phone-a"));
    }
}

#[test]
fn untrusted_ids_cannot_inject_prompt_markup_or_shell_arguments() {
    for id in [
        "",
        "a;echo evil",
        "$(evil)",
        "a`evil`",
        "</sheprd>",
        &"a".repeat(257),
    ] {
        let note = Context::default()
            .before_prompt(Some(phone(id)), "hi")
            .unwrap();
        assert!(!note.contains("--device"));
        assert!(split(&note).is_some());
    }
}

#[test]
fn an_unidentified_message_after_replay_clears_the_old_phone() {
    let first = Context::default()
        .before_prompt(Some(phone("phone-a")), "one")
        .unwrap();
    let mut restored = Context::default();
    restored.heard(&first);
    let note = restored
        .before_prompt(None, "from an older client")
        .unwrap();
    assert!(note.contains("No phone target is identified"));
    assert!(!note.contains("phone-a"));
}
