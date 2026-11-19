use super::*;

/// A local workspace reveals its folder from the header, beside the mute
/// toggle, so the action adds no row that would push the pull request card
/// out of a short window.
#[gpui::test]
fn a_local_workspace_reveals_its_folder_from_the_header(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.open_workspace_menu("w3", gpui::Point::default(), window, cx);
        });
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        let items = view.workspace_items();
        assert_eq!(items[0].0, WorkspaceMenuAction::Mute);
        assert_eq!(items[1].0, WorkspaceMenuAction::RevealInFileManager);
        assert!(items[1].0.in_header());
        // The header draws it, so the rows below leave it out.
        let (_, rest) = crate::menu::workspace::popover::layout(&items);
        assert!(
            !rest
                .iter()
                .any(|(action, _)| *action == WorkspaceMenuAction::RevealInFileManager)
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let selector = cx.update(|_, cx| {
        let label = view.read(cx).workspace_items()[1].1;
        format!("workspace-menu-{}", label.english()).leak()
    });
    assert!(cx.debug_bounds(selector).is_some());
}

/// Another machine's folders are not this computer's to reveal.
#[gpui::test]
fn a_remote_workspace_offers_no_reveal(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Ssh {
                target: "test.invalid".into(),
                session: "default".into(),
            };
            view.open_workspace_menu("w3", gpui::Point::default(), window, cx);
            assert!(
                !view
                    .workspace_items()
                    .iter()
                    .any(|(action, _)| *action == WorkspaceMenuAction::RevealInFileManager)
            );
        });
    });
}
