//! Settings > Cloud devices with providers turned off under `[features]`:
//! their tabs and cards go, and with every provider off so does New device
//! setup, from the page and from search alike.
use super::*;
use crate::cloud::{CloudProvider, enable_for_test};
use gpui::{px, size};

fn draw_cloud_devices(view: &Entity<SettingsWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.open_cloud_devices(cx));
    view.update(cx, |view, _| view.section = Section::CloudDevices);
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn tab(provider: CloudProvider) -> &'static str {
    format!("cloud-tab-{}", provider.key()).leak()
}

fn offers_setup(view: &mut SettingsWindow, cx: &mut Context<SettingsWindow>) -> bool {
    view.set_search_query("New device setup", cx);
    view.visible_results()
        .iter()
        .any(|index| catalog::CATALOG[*index].id == "cloud-setup")
}

#[gpui::test]
fn a_provider_turned_off_has_no_tab(cx: &mut TestAppContext) {
    let [off, on, ..] = CloudProvider::ALL else {
        return;
    };
    let _enabled = enable_for_test(&[*on]);
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(3000.)));
    view.read_with(cx, |view, _| assert_eq!(view.cloud_tab, *on));
    draw_cloud_devices(&view, cx);
    assert!(cx.debug_bounds("cloud-setup").is_some());
    assert!(cx.debug_bounds(tab(*off)).is_none());
    view.update(cx, |view, cx| assert!(offers_setup(view, cx)));
}

#[gpui::test]
fn every_provider_off_leaves_only_a_note(cx: &mut TestAppContext) {
    let _enabled = enable_for_test(&[]);
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(3000.)));
    draw_cloud_devices(&view, cx);
    assert!(cx.debug_bounds("cloud-accounts").is_some());
    assert!(cx.debug_bounds("cloud-setup").is_none());
    for &provider in CloudProvider::ALL {
        assert!(cx.debug_bounds(tab(provider)).is_none(), "{provider:?}");
    }
    view.update(cx, |view, cx| {
        assert!(
            !offers_setup(view, cx),
            "search must not offer a row the page hides"
        );
    });
}
