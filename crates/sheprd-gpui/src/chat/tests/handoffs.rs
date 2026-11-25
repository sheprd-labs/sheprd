//! The divider where another agent took over a chat's conversation.

use sheprd_chat_protocol::{Agent, HandedOver};

fn handoff(kept: usize, omitted: usize, resumed: bool) -> HandedOver {
    HandedOver {
        from: Some(Agent::Claude),
        kept,
        omitted,
        resumed,
        to: Agent::Codex,
    }
}

/// The divider's text without Fluent's isolation marks.
fn text(handoff: &HandedOver) -> String {
    crate::chat::text::handoff(handoff).replace(['\u{2068}', '\u{2069}'], "")
}

#[test]
fn a_handoff_says_who_took_over_and_how_much_it_was_handed() {
    assert_eq!(
        text(&handoff(42, 0, false)),
        "Continued in Codex: 42 entries handed over"
    );
    assert_eq!(
        text(&handoff(1, 0, true)),
        "Back to Codex: 1 new entry handed over"
    );
    assert_eq!(
        text(&handoff(42, 17, false)),
        "Continued in Codex: 42 entries handed over, 17 left out for length"
    );
}
