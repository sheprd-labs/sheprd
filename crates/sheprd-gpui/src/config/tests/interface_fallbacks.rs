use super::*;
use crate::config::fonts::{interface_fallbacks, symbol_fallbacks};

fn owned(families: &[&str]) -> Vec<String> {
    families.iter().map(|family| (*family).to_owned()).collect()
}

#[test]
fn symbols_lead_then_cjk_then_patched_text_faces() {
    let detected = symbol_fallbacks(owned(&[
        "JetBrainsMono Nerd Font",
        "Symbols Nerd Font",
        "D2CodingLigature Nerd Font Mono",
    ]));
    assert_eq!(
        interface_fallbacks(&detected, &owned(&["Apple SD Gothic Neo"])),
        owned(&[
            "Symbols Nerd Font",
            "Apple SD Gothic Neo",
            "D2CodingLigature Nerd Font Mono",
            "JetBrainsMono Nerd Font",
        ])
    );
}

#[test]
fn the_split_does_not_depend_on_the_detected_order() {
    // A patched face listed before a symbols face still follows the CJK faces.
    let detected = owned(&["Hack Nerd Font", "symbols nerd font mono"]);
    assert_eq!(
        interface_fallbacks(&detected, &owned(&["Noto Sans CJK KR"])),
        owned(&[
            "symbols nerd font mono",
            "Noto Sans CJK KR",
            "Hack Nerd Font"
        ])
    );
}

#[test]
fn without_cjk_faces_the_icon_cascade_is_unchanged() {
    let detected = owned(&["Symbols Nerd Font Mono", "Hack Nerd Font Mono"]);
    assert_eq!(interface_fallbacks(&detected, &[]), detected);
    assert!(interface_fallbacks(&[], &[]).is_empty());
}

#[test]
fn configured_interface_fallbacks_keep_their_order() -> anyhow::Result<()> {
    let mut config = Config::parse(
        "language = 'ko'\n[ui]\nfallback = ['Hack Nerd Font', 'Symbols Nerd Font']\n\
         [sidebar]\nfallback = []",
    )?;
    config.resolve_fonts_with(
        || owned(&["Symbols Nerd Font", "Hack Nerd Font", "Noto Sans CJK KR"]),
        |_| false,
    );
    assert_eq!(
        config.ui.fallbacks.as_deref(),
        Some(owned(&["Hack Nerd Font", "Symbols Nerd Font"]).as_slice())
    );
    assert_eq!(config.sidebar.fallbacks.as_deref(), Some([].as_slice()));
    Ok(())
}
