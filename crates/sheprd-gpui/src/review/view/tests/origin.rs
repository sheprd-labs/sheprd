use super::*;

#[gpui::test]
fn retained_review_notes_cannot_target_a_replacement_session(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, Some("working"));
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    note(&view, cx, line(3), "Original review.");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.boot_id = "replacement-session".into();
            view.live.snapshot = Some(Arc::new(snapshot));
            view.send_review(the(view), cx);
            assert_eq!(view.deliveries.len(), 0);
        });
        let target = crate::browser::FeedbackKey {
            scope: crate::browser::scope(&view.read(cx).endpoints[0]),
            pane_id: "w0:p1".into(),
        };
        assert!(
            cx.default_global::<crate::browser::Feedback>()
                .take(&target)
                .is_none()
        );
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(text.contains("Original review."));
    });
}

#[gpui::test]
fn a_background_review_belongs_to_the_callers_daemon(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    view.update(cx, |view, cx| {
        let local = &mut view.endpoints[0].live;
        local.snapshot = Some(Arc::new(crate::sidebar::layout_tests::snapshot(6)));
        local.local_daemon_peer = true;
        local.status = crate::state::ConnectionStatus::Connected;
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "ssh:other".into(),
            "other".into(),
            sheprd_client::ConnectTarget::Ssh {
                target: "other".into(),
                session: "work".into(),
            },
            true,
        ));
        view.selected_endpoint = 1;
        let id = view.background_review(0, "w3", None, None, cx).unwrap();
        let origin = view.reviews[&id].origin.clone().unwrap();
        assert!(!origin.current(view));
        view.selected_endpoint = 0;
        std::mem::swap(&mut view.live, &mut view.endpoints[0].live);
        assert!(origin.current(view));
    });
}
