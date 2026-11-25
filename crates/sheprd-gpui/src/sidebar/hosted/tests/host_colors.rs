use super::*;

#[gpui::test]
fn host_colors_and_selection_strength_refresh_the_hosted_rows(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        let label = view.endpoints[view.selected_endpoint].label.clone();
        let before = view.hosted_stamp();
        view.config.sidebar_style.hosts.insert(label, 0xff356f);
        assert!(view.hosted_stamp() != before);
        let device = &view.hosted_state(cx).main["devices"][0];
        assert_eq!(device["wash"], "#ff356f1a");
        assert_eq!(device["opacity"], 1.);

        let before = view.hosted_stamp();
        view.config.sidebar_style.select = crate::config::SelectMode::GroupDim;
        assert!(view.hosted_stamp() != before);
        assert_eq!(
            view.hosted_state(cx).main["devices"][0]["wash"],
            "#ff356f38"
        );
    });
}
