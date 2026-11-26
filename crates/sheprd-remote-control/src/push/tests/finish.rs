//! A terminal agent's finish says where, what was asked, and what it did.

use super::*;
use crate::push::{decide_terminal, finish::generated, recapped};
use sheprd_client::protocol::AgentStatus;

fn finish(workspace: &'static str, tab: &'static str, privacy: Privacy) -> Notification {
    let agent = SeenAgent {
        chat: Chat {
            agent: "Antigravity",
            pane: "w1-p3",
            title: tab,
            workspace,
        },
        status: AgentStatus::Done,
        task: Some("Fix the flaky socket test"),
    };
    let moved = Moved {
        pane: "w1-p3".to_owned(),
        from: AgentStatus::Working,
        to: Some(AgentStatus::Done),
        started_ms: None,
    };
    decide_terminal(&agent, &moved, privacy).unwrap()
}

const RECAP: &str =
    "Fixed it by waiting for the socket to accept before\nreading; all 214 tests pass.";

#[test]
fn says_where_what_was_asked_and_what_was_done() {
    let told = recapped(
        finish("herdr-gpui", "server", Privacy::Excerpt),
        Some("Fix the flaky socket test"),
        RECAP,
    );
    assert_eq!(told.title, "herdr-gpui");
    assert_eq!(told.subtitle, "Fix the flaky socket test");
    assert_eq!(
        told.body,
        "Fixed it by waiting for the socket to accept before reading; all 214 tests pass."
    );
    assert_eq!(told.kind, Kind::TerminalDone);
}

#[test]
fn without_a_recap_the_task_is_the_body() {
    let finished = finish("herdr-gpui", "server", Privacy::Excerpt);
    assert_eq!(finished.subtitle, "");
    assert_eq!(finished.body, "Fix the flaky socket test");
    let told = recapped(finished.clone(), Some("Fix the flaky socket test"), "  \n ");
    assert_eq!(told, finished, "an empty screen changes nothing");
}

#[test]
fn a_generated_workspace_name_gives_way_to_the_tab() {
    let finished = finish("worktree-green-forest-0c5b", "fix-tabs", Privacy::Excerpt);
    assert_eq!(finished.title, "fix-tabs");
    let finished = finish("green-meadow-a5eb", "", Privacy::Excerpt);
    assert_eq!(finished.title, "Antigravity", "nothing else names it");
}

#[test]
fn generic_privacy_says_only_where_and_done() {
    let finished = finish("herdr-gpui", "server", Privacy::Generic);
    assert_eq!(
        (
            finished.title.as_str(),
            finished.subtitle.as_str(),
            finished.body.as_str()
        ),
        ("herdr-gpui", "", "Done")
    );
}

#[test]
fn a_recap_never_rewrites_a_question() {
    let mut asking = finish("herdr-gpui", "server", Privacy::Excerpt);
    asking.kind = Kind::TerminalAsking;
    assert_eq!(recapped(asking.clone(), None, RECAP), asking);
}

#[test]
fn generated_names_are_two_words_and_four_hex_digits() {
    for name in [
        "green-meadow-a5eb",
        "worktree-green-forest-0c5b",
        "sheprd/calm-river-0042",
    ] {
        assert!(generated(name), "{name}");
    }
    for name in [
        "herdr-gpui",
        "fix-tabs",
        "fix-tabs-v2",
        "Green-Meadow-a5eb",
        "a5eb",
    ] {
        assert!(!generated(name), "{name}");
    }
}
