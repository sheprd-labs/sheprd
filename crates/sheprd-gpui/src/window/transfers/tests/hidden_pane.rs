use super::*;

/// How the pane a file was dropped on stops being the one shown.
#[derive(Clone, Copy, Debug)]
enum Hidden {
    /// Another tab is shown.
    OtherTab,
    /// A popup opened over it.
    Popup,
    /// A tab switch is waiting for its frame, so the shown tab is not
    /// ready for input when the copy finishes.
    Navigating,
    /// A notification's pane is being brought forward.
    Toast,
}

/// None of these cancels the copy: the pane still exists, so the copied
/// paths are pasted into it by its ID.
#[gpui::test]
fn a_copy_keeps_going_and_pastes_into_a_pane_another_tab_hides(cx: &mut TestAppContext) {
    for hidden in [
        Hidden::OtherTab,
        Hidden::Popup,
        Hidden::Navigating,
        Hidden::Toast,
    ] {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let mut peer = Peer::new();
        view.update(cx, |view, cx| {
            peer.prepare(view, cx);
            view.start_file_transfer_with(
                InputTarget::Pane("w1:p1".into()),
                vec!["source".into()],
                |_, _, _, progress| {
                    progress(9, 9);
                    Ok(vec![REMOTE.into()])
                },
                |_, _| panic!("accepted upload was removed"),
                cx,
            );
            match hidden {
                Hidden::OtherTab => {
                    Arc::make_mut(view.live.surface.as_mut().unwrap())
                        .panes
                        .clear();
                }
                Hidden::Popup => popup(view, "popup"),
                Hidden::Navigating => {
                    view.live.supports_surface = true;
                    // The switch takes the window's state from the inbox.
                    *view.endpoints[0].connection.inbox.lock().unwrap() = view.live.clone();
                    view.dispatch_navigation(crate::navigation::NavigationTarget::Tab("w1:t2"), cx);
                    assert!(!view.input_ready(), "the switch waits for its frame");
                }
                Hidden::Toast => view.pending_toast = Some(1),
            }
            view.poll_file_transfer(cx);
            let transfer = view.file_transfer.as_ref().unwrap();
            assert!(!transfer.cancelled.load(Ordering::Acquire), "{hidden:?}");
            assert!(!transfer.shown.2, "{hidden:?}");
        });
        cx.run_until_parked();
        let pasted = loop {
            // A tab switch sends its own requests first.
            if let message @ ClientMessage::ClientShellPaneInput { .. } = peer.receive() {
                break message;
            }
        };
        assert_eq!(
            pasted,
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![ClientPaneInputEvent::Paste(
                    "/tmp/herdr-upload.ABCDEF123456/it\\'s\\ a\\ file".into(),
                )],
            },
            "{hidden:?}"
        );
        view.read_with(cx, |view, _| {
            assert!(view.file_transfer.is_none(), "{hidden:?}");
            assert_eq!(
                view.endpoints[0].toasts.entries.back().unwrap().1.title,
                "Copy complete",
                "{hidden:?}"
            );
        });
        peer.sentinel(&view, cx);
    }
}
