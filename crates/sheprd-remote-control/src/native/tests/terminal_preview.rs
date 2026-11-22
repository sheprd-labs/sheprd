//! A terminal agent's row says what it last said, as its screen showed it,
//! and a phone following home hears when that changes.

use super::*;
use wire::View;

fn agent(home: &Home) -> &model::TerminalAgent {
    home.workspaces[0].terminals[0].agent.as_ref().unwrap()
}

#[tokio::test]
async fn home_sends_a_terminal_agents_preview_when_it_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let mut session = Session::start(state.clone());
    session
        .send(ClientMessage::Subscribe {
            background: false,
            view: View::Home,
            since: None,
            size: None,
        })
        .await;
    let ServerMessage::Home { home } = session.next().await else {
        panic!("not home");
    };
    assert_eq!(agent(&home).preview, None);
    let wire = serde_json::to_value(agent(&home)).unwrap();
    assert!(wire.get("preview").is_none(), "{wire}");

    state.previews.set_for_tests(HashMap::from([
        ("p2".into(), "Fixed the build.\nPushed it.".into()),
        ("gone".into(), "A pane no tab lists".into()),
    ]));
    let ServerMessage::Home { home } = session.next().await else {
        panic!("not home");
    };
    assert_eq!(
        agent(&home).preview.as_deref(),
        Some("Fixed the build.\nPushed it.")
    );
    let wire = serde_json::to_value(agent(&home)).unwrap();
    assert_eq!(wire["preview"], "Fixed the build.\nPushed it.");
}
