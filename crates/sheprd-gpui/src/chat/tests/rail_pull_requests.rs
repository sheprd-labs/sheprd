//! The rail's Pull requests section: shown once the agent worked on one,
//! a row for each, and a click opens the pull request.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::rail_folds::RailSection;
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{Change, PullRequest, PullRequestAction};
use sheprd_client::ConnectTarget;

const URL: &str = "https://github.com/sheprd-labs/herdr-gpui/pull/1446";

/// The section's header, by the selector `render_chat_rail_section` gives it.
fn section() -> &'static str {
    let index = RailSection::PullRequests.index();
    Box::leak(format!("chat-rail-section-{index}").into_boxed_str())
}

fn idle(entries: Vec<Entry>) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = entries;
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn a_pull_request_the_agent_worked_on_gets_a_section_and_opens_on_click(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle(vec![agent(1, "Opening it.")]), cx);
    assert!(cx.debug_bounds(section()).is_none(), "nothing to list yet");

    let mut pull = PullRequest::parse(URL).unwrap();
    pull.actions = vec![PullRequestAction::Created];
    send(
        &mut stream,
        &ServerMessage::Change {
            change: Change::PullRequest { pull_request: pull },
            cursor: cursor(1, 1),
        },
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-rail-pull-request-0").is_some()
    });
    assert!(
        cx.debug_bounds("chat-rail").is_some(),
        "the rail shows for it"
    );
    assert!(cx.debug_bounds(section()).is_some());

    // Nothing lists it in the app, so its page opens.
    let row = cx.debug_bounds("chat-rail-pull-request-0").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}
