#![allow(clippy::unwrap_used)]
#[test]
fn long_titles_are_shortened_on_a_character_boundary() {
    assert_eq!(super::tab_label("Example Domain"), "Example Domain");
    let long = "Rust Programming Language — Official Site";
    assert_eq!(
        super::tab_label(long),
        "Rust Programming Language \u{2014}\u{2026}"
    );
    assert_eq!(super::tab_label(&"é".repeat(40)).chars().count(), 29);
}

#[gpui::test]
fn a_local_project_survives_its_workspace_closing_from_another_client(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot: sheprd_client::protocol::ClientShellSnapshot = serde_json::from_str(
            include_str!("../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"),
        )
        .unwrap();
        snapshot.workspaces[0].new_workspace_cwd = "/work/Chrome/project".into();
        snapshot.workspaces[0].label = "project".into();
        view.live.snapshot = Some(std::sync::Arc::new(snapshot));
        view
    });
    let scope = view.read_with(cx, |view, _| {
        crate::browser::scope(&view.endpoints[view.selected_endpoint])
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.forget_closed_workspaces(cx));
    });
    cx.update(|_, cx| {
        crate::browser::Store::update(cx, |store| {
            assert_eq!(store.closed_projects(&scope).count(), 0);
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.revision += 1;
            snapshot.workspaces.clear();
            snapshot.tabs.clear();
            snapshot.panes.clear();
            view.live.snapshot = Some(std::sync::Arc::new(snapshot));
            view.forget_closed_workspaces(cx);
        });
    });
    cx.update(|_, cx| {
        crate::browser::Store::update(cx, |store| {
            let project = store.closed_projects(&scope).next().unwrap();
            assert_eq!(project.cwd, "/work/Chrome/project");
            assert_eq!(project.label, "project");
        });
    });
}

#[gpui::test]
fn a_local_project_is_retained_while_another_endpoint_is_selected(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut local: sheprd_client::protocol::ClientShellSnapshot = serde_json::from_str(
            include_str!("../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"),
        )
        .unwrap();
        local.workspaces[0].new_workspace_cwd = "/work/Chrome/project".into();
        local.workspaces[0].label = "project".into();
        view.live.snapshot = Some(std::sync::Arc::new(local.clone()));
        view.endpoints[0].live.snapshot = Some(std::sync::Arc::new(local));

        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Test host".into(),
            sheprd_client::ConnectTarget::Ssh {
                target: "test.invalid".into(),
                session: "default".into(),
            },
            true,
        );
        remote.never_connect();
        remote.live.snapshot = Some(std::sync::Arc::new(crate::sidebar::layout_tests::snapshot(
            1,
        )));
        view.endpoints.push(remote);
        view.selected_endpoint = 1;
        view.live = view.endpoints[1].live.clone();
        view
    });
    let scope = view.read_with(cx, |view, _| crate::browser::scope(&view.endpoints[0]));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.forget_closed_workspaces(cx));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.endpoints[0].live.snapshot.as_ref().unwrap()).clone();
            snapshot.revision += 1;
            snapshot.workspaces.clear();
            snapshot.tabs.clear();
            snapshot.panes.clear();
            view.endpoints[0].live.snapshot = Some(std::sync::Arc::new(snapshot));
            view.forget_closed_workspaces(cx);
        });
    });
    cx.update(|_, cx| {
        crate::browser::Store::update(cx, |store| {
            assert_eq!(
                store.closed_projects(&scope).next().unwrap().cwd,
                "/work/Chrome/project"
            );
        });
    });
}
