use crate::context::prompts::tests::*;

const HARDWARE_KEYS: &str = "They cannot touch this computer: a YubiKey";

/// Every template a person at `origin` is sent, rendered and joined.
fn note(origin: Origin) -> String {
    let prompts = Prompts::default();
    std::iter::once(Kind::of(origin.clone()))
        .chain(Kind::after(origin.clone()))
        .map(|kind| prompts.render(kind, &facts(), Some(origin.clone())))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_person_away_from_this_computer_is_said_once_to_have_no_hardware_key() {
    for (device, via) in [
        (Device::PhoneApp, Via::Relay),
        (Device::PhoneWeb, Via::Lan),
        (Device::Desktop, Via::Remote),
        (Device::Terminal, Via::Remote),
        (Device::Other, Via::Local),
    ] {
        let text = note(Origin::new(device, via));
        assert_eq!(
            text.matches(HARDWARE_KEYS).count(),
            1,
            "{device:?} via {via:?}: {text}"
        );
        assert!(text.contains("credential.helper='!gh auth git-credential'"));
        assert!(text.contains("never turn signing off"));
    }
}

#[test]
fn a_person_at_this_computer_is_not_told_about_hardware_keys() {
    for device in [Device::Desktop, Device::Terminal] {
        let text = note(Origin::new(device, Via::Local));
        assert!(!text.contains(HARDWARE_KEYS), "{device:?}: {text}");
    }
}
