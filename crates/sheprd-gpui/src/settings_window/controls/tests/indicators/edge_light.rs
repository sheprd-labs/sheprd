use super::*;

#[gpui::test]
fn edge_light_controls_save_hex_auto_width_and_trail(cx: &mut TestAppContext) {
    let (view, cx, edits) = indicators_page(cx);
    let click = |id, cx: &mut VisualTestContext| {
        let bounds = cx.debug_bounds(id).unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    };
    click("edge-light-hex", cx);
    cx.simulate_input("#29bAc3");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        std::mem::take(&mut *edits.lock().unwrap()),
        [Preference::EdgeLightColor(Some(0x29bac3))]
    );
    // The fixture reloads defaults; show the saved colour before resetting it.
    view.update(cx, |view, cx| {
        view.config.edge_light.color = Some(0x29bac3);
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    click("edge-light-auto", cx);
    click("edge-light-width-control-plus", cx);
    click("edge-light-trail-control-minus", cx);
    assert_eq!(
        *edits.lock().unwrap(),
        [
            Preference::EdgeLightColor(None),
            Preference::EdgeLightWidth(6),
            Preference::EdgeLightTrail(15)
        ]
    );
}

#[gpui::test]
fn typed_hex_stays_open_until_a_busy_save_finishes(cx: &mut TestAppContext) {
    let (view, cx, edits) = indicators_page(cx);
    let bounds = cx.debug_bounds("edge-light-hex").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_input("#29bAc3");
    view.update(cx, |view, cx| {
        view.saving = true;
        cx.notify();
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_section(Section::Sidebar, window, cx)
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.section, Section::Indicators);
        assert!(
            view.hex_edit.is_some(),
            "busy save must preserve the hex draft"
        );
    });
    assert!(
        edits.lock().unwrap().is_empty(),
        "busy save must not drop a write"
    );

    view.update(cx, |view, cx| {
        view.saving = false;
        cx.notify();
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.hex_edit.is_none()));
    assert_eq!(
        *edits.lock().unwrap(),
        [Preference::EdgeLightColor(Some(0x29bac3))]
    );
}
