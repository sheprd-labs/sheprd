//! The `guests` case: a chat shared through an external tool. A guest's
//! message the person allowed is named above its words, and another waits
//! for the person with every word it would send.

use sheprd_chat_protocol::{
    Entry, EntryId, EntryKind, GUEST_ALLOW, GUEST_DENY, Guest, GuestPrompt, Permission,
    PermissionId, PermissionKind, PermissionOption, Transcript,
};

fn user(id: u64, guest: Option<Guest>, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            guest,
            text: text.into(),
            files: 0,
            note: None,
        },
    }
}

pub(super) fn apply(transcript: &mut Transcript) {
    let bob = Guest::new("Bob", "tma"); // i18n: data
    transcript.entries = vec![
        user(1, None, "Fix the flaky parser test, then wait for Bob's review."), // i18n: data
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "The test raced the file watcher. It now waits for the first event, and it passed 200 runs in a row.".into(), // i18n: data
            },
        },
        user(3, Some(bob.clone()), "Looks good. Could you also cover the empty input case?"), // i18n: data
        Entry {
            id: EntryId(4),
            kind: EntryKind::Agent {
                text: "Added `parses_empty_input`, which checks the parser returns an empty document instead of an error.".into(), // i18n: data
            },
        },
    ];
    let text = "One more thing: run the benchmarks before and after,\nand paste both numbers here so I can compare them.".to_owned(); // i18n: data
    transcript.permissions = vec![Permission {
        call_id: None,
        id: PermissionId(1),
        options: vec![
            PermissionOption {
                id: GUEST_ALLOW.into(),
                kind: PermissionKind::AllowOnce,
                name: "Send to the agent".into(), // i18n: data
            },
            PermissionOption {
                id: GUEST_DENY.into(),
                kind: PermissionKind::RejectOnce,
                name: "Decline".into(), // i18n: data
            },
        ],
        question: None,
        title: format!("{} asks to send:\n{text}", bob.label()), // i18n: data
        guest: Some(Box::new(GuestPrompt { guest: bob, text })),
    }];
}
