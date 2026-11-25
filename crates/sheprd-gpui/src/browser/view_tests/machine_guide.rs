use super::*;
use crate::machine::{Name, guide};
use gpui::{Modifiers, MouseButton};

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    draw(cx);
}

/// The guide docks beside a machine's tab, takes the step only the user can
/// vouch for from a click, and its close button puts the tab back whole.
#[gpui::test]
fn the_setup_guide_sits_beside_the_machine_and_closes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(gpui::px(1600.), gpui::px(800.)));
    let name = Name::try_from("guide-test".to_owned()).unwrap();
    cx.update(|window, cx| {
        let scope = scope(&view.read(cx).endpoints[0]);
        let location = Some(Location::Machine { name: name.clone() });
        let id = Store::update(cx, |store| store.open(scope, "w0", location, None)).unwrap();
        view.update(cx, |view, cx| view.show_browser_tab(id, window, cx));
        guide::begin(&name, guide::Opening::Shown, cx);
    });
    draw(cx);
    draw(cx);

    let panel = cx.debug_bounds("machine-guide").unwrap();
    // Docked at the right, at its own width; the machine takes the rest.
    assert_eq!(panel.size.width, gpui::px(280.));
    let window_right = cx.update(|window, _| window.viewport_size().width);
    assert!(panel.right() <= window_right && panel.left() > window_right / 2.);
    assert!(cx.debug_bounds("machine-guide-signed-in").is_some());

    click(cx, "machine-guide-signed-in");
    assert!(cx.debug_bounds("machine-guide-signed-in").is_none());
    let step = cx.update(|_, _| guide::shown(&name).unwrap().step);
    assert_eq!(step, guide::Step::Install);

    click(cx, "machine-guide-close");
    assert!(cx.debug_bounds("machine-guide").is_none());
    assert!(cx.update(|_, _| guide::shown(&name)).is_none());
}
