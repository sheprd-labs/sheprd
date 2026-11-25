use super::*;
use crate::control::{
    Response,
    labels::{Edit, Target},
};

#[tokio::test]
async fn labels_follow_home_subscriptions_and_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let layout = layout(EndpointId::random());
    let state = offline(dir.path(), layout.clone());
    let mut session = Session::start(state.clone());
    session
        .send(ClientMessage::Subscribe {
            background: false,
            view: wire::View::Home,
            since: None,
            size: None,
        })
        .await;
    assert!(matches!(session.next().await, ServerMessage::Home { .. }));
    let stop = tokio::sync::watch::channel(false).0;
    let edit = |id: &str, worktree, change| {
        let response = crate::server::answer_for_tests(
            &state,
            &stop,
            crate::control::Request::Labels {
                target: Target::Pane(id.into()),
                worktree,
                edit: change,
            },
        );
        let Response::Labels { tags } = response else {
            panic!("unexpected {response:?}")
        };
        tags
    };
    assert_eq!(edit("p1", true, Edit::Add("Release".into())), ["Release"]);
    let ServerMessage::Home { home: updated } = session.next().await else {
        panic!("expected home")
    };
    assert_eq!(updated.workspaces[0].labels, ["Release"]);
    assert_eq!(edit("p1", false, Edit::Add("release".into())), ["release"]);
    assert_eq!(edit("p2", false, Edit::Add("triage".into())), ["triage"]);
    assert_eq!(
        edit(
            "p1",
            false,
            Edit::Rename {
                old: "Release".into(),
                new: "UI".into()
            }
        ),
        ["UI"]
    );
    assert!(edit("p2", true, Edit::Remove("RELEASE".into())).is_empty());
    let reloaded = offline(dir.path(), layout);
    let row = &home(&reloaded, &DeviceId("test".into())).workspaces[0];
    assert!(row.labels.is_empty());
    assert_eq!(row.chats[0].labels, ["UI"]);
    assert_eq!(row.terminals[0].agent.as_ref().unwrap().labels, ["triage"]);
}

#[tokio::test]
async fn concurrent_labels_keep_both_edits_and_ignore_other_boots() {
    let dir = tempfile::tempdir().unwrap();
    let mut layout = layout(EndpointId::random());
    let state = offline(dir.path(), layout.clone());
    let add = |tag: &'static str| {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            state.labels.answer(
                &state.hub,
                Target::Pane("p2".into()),
                false,
                Edit::Add(tag.into()),
            )
        })
    };
    let (one, two) = tokio::join!(add("UI"), add("API"));
    for answer in [one, two] {
        assert!(matches!(answer.unwrap(), Response::Labels { .. }));
    }
    let device = DeviceId("test".into());
    let current = home(&state, &device);
    let mut tags = current.workspaces[0].terminals[0]
        .agent
        .as_ref()
        .unwrap()
        .labels
        .clone();
    tags.sort();
    assert_eq!(tags, ["API", "UI"]);
    layout.workspaces[0].terminals[0]
        .agent
        .as_mut()
        .unwrap()
        .boot = "new boot".into();
    let rebooted = offline(dir.path(), layout);
    let current = home(&rebooted, &device);
    assert!(
        current.workspaces[0].terminals[0]
            .agent
            .as_ref()
            .unwrap()
            .labels
            .is_empty()
    );
}

#[tokio::test]
async fn unlisted_agents_do_not_share_worktree_labels() {
    let dir = tempfile::tempdir().unwrap();
    let mut layout = layout(EndpointId::random());
    let mut checkout = layout.workspaces[0].clone();
    checkout.chats.clear();
    checkout.terminals.clear();
    // Both rows advertise /work, even though the unlisted terminal is elsewhere.
    let unlisted = &mut layout.workspaces[0];
    unlisted.id = "core:terminals".into();
    unlisted.terminals[0].cwd = Some("/elsewhere".into());
    unlisted.terminals[0].panes[0].cwd = Some("/elsewhere".into());
    layout.workspaces.push(checkout);
    let state = offline(dir.path(), layout);
    let add = |target, worktree, name: &str| {
        state
            .labels
            .answer(&state.hub, target, worktree, Edit::Add(name.into()))
    };
    assert!(matches!(
        add(Target::Workspace("w1".into()), true, "Release"),
        Response::Labels { .. }
    ));
    for target in [
        Target::Workspace("core:terminals".into()),
        Target::Pane("p1".into()),
        Target::Pane("p2".into()),
    ] {
        assert!(matches!(add(target, true, "Mixed"), Response::Error { .. }));
    }
    assert!(matches!(
        add(Target::Pane("p2".into()), false, "Review"),
        Response::Labels { .. }
    ));
    let home = home(&state, &DeviceId("test".into()));
    let group = home
        .workspaces
        .iter()
        .find(|w| w.id == "core:terminals")
        .unwrap();
    assert!(group.labels.is_empty());
    assert_eq!(
        group.terminals[0].agent.as_ref().unwrap().labels,
        ["Review"]
    );
    assert_eq!(
        home.workspaces
            .iter()
            .find(|w| w.id == "w1")
            .unwrap()
            .labels,
        ["Release"]
    );
}

#[tokio::test]
async fn worktree_labels_are_shared_by_checkout_within_a_wsl_distribution() {
    let dir = tempfile::tempdir().unwrap();
    let mut layout = layout(EndpointId::random());
    let native = &layout.workspaces[0];
    let mut workspaces = vec![native.clone()];
    for session in ["dev@Ubuntu", "review@Ubuntu", "dev@Debian"] {
        let mut workspace = native.clone();
        workspace.id = format!("herdr:{session}:w1");
        workspace.session = Some(session.into());
        workspace.chats.clear();
        workspace.terminals.clear();
        workspaces.push(workspace);
    }
    layout.workspaces = workspaces;
    let state = offline(dir.path(), layout);
    let response = state.labels.answer(
        &state.hub,
        Target::Workspace("herdr:dev@Ubuntu:w1".into()),
        true,
        Edit::Add("Release".into()),
    );
    assert!(matches!(response, Response::Labels { .. }));
    let home = home(&state, &DeviceId("test".into()));
    assert_eq!(home.workspaces.len(), 4);
    for workspace in home.workspaces {
        match workspace.id.as_str() {
            "herdr:dev@Ubuntu:w1" | "herdr:review@Ubuntu:w1" => {
                assert_eq!(workspace.labels, ["Release"])
            }
            "w1" | "herdr:dev@Debian:w1" => assert!(workspace.labels.is_empty()),
            id => panic!("unexpected workspace {id}"),
        }
    }
}
