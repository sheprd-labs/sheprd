//! Phones speak for the person: no guest's words go through the daemon's
//! shared connection to a chat.

use sheprd_chat_protocol::{Command, Guest, Origin};

use crate::hub::chat::relays_guest;

#[test]
fn guest_prompts_never_go_through_the_phones_connection() {
    let guest = Guest::new("Bob", "tma");
    assert!(relays_guest(&Command::GuestPrompt {
        text: "hi".to_owned(),
        guest: guest.clone(),
    }));
    assert!(relays_guest(&Command::Prompt {
        text: "hi".to_owned(),
        attachments: Vec::new(),
        origin: Some(Origin::guest(guest)),
    }));
    assert!(!relays_guest(&Command::Prompt {
        text: "hi".to_owned(),
        attachments: Vec::new(),
        origin: None,
    }));
    assert!(!relays_guest(&Command::Cancel));
}
