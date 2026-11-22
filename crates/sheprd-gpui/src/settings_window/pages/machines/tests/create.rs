use super::*;
use crate::machine::install::{self, Progress};
use gpui::{Modifiers, MouseButton};

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn name(text: &str) -> Name {
    Name::try_from(text.to_owned()).unwrap()
}

/// With nothing installing, the first row offers to make a machine, above
/// the machines there are.
#[gpui::test]
fn the_page_offers_to_make_a_machine_first(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx, vec![listing("one", false)], None);
    let create = cx.debug_bounds("machine-create").unwrap();
    let first = cx.debug_bounds("machine-one-open").unwrap();
    assert!(create.bottom() < first.top());
    assert!(cx.debug_bounds("machine-create-bar").is_none());
    assert!(cx.debug_bounds("machine-create-cancel").is_none());
}

/// A download shows how far it has got, and can be stopped; it cannot be
/// started twice.
#[gpui::test]
fn a_download_shows_its_progress_and_a_stop_button(cx: &mut TestAppContext) {
    install::pretend(
        name("default"),
        Progress::Downloading {
            version: "26.1".to_owned(),
            done: 5_000_000_000,
            total: 15_000_000_000,
        },
    );
    let (_view, cx) = page(cx, Vec::new(), None);
    assert!(cx.debug_bounds("machine-create").is_none());
    assert!(cx.debug_bounds("machine-create-cancel").is_some());
    let bar = cx.debug_bounds("machine-create-bar").unwrap();
    let row = cx.debug_bounds("machine-create-row").unwrap();
    assert!(bar.left() >= row.left() && bar.right() <= row.right());
}

/// A failed install says why, offers another try, and goes when dismissed.
#[gpui::test]
fn a_failed_install_offers_another_try_and_can_be_dismissed(cx: &mut TestAppContext) {
    install::pretend(name("default"), Progress::Failed("No network".to_owned()));
    let (_view, cx) = page(cx, Vec::new(), None);
    assert!(cx.debug_bounds("machine-create").is_some());
    assert!(cx.debug_bounds("machine-create-cancel").is_none());
    click(cx, "machine-create-dismiss");
    assert_eq!(install::progress(), None);
    assert!(cx.debug_bounds("machine-create-dismiss").is_none());
}

#[test]
fn progress_is_a_fraction_of_what_is_known() {
    use super::super::create::fraction;
    assert_eq!(fraction(5, 0), 0.);
    assert_eq!(fraction(5, 10), 0.5);
    assert_eq!(fraction(15, 10), 1.);
}

/// Try again makes the machine its failed row names, not the next default.
#[test]
fn a_retry_keeps_the_failed_name() {
    use super::super::create::new_name;
    let taken = |text: &str| text == "default";
    assert_eq!(
        new_name(None, Some(name("work")), taken),
        Some(name("work"))
    );
    assert_eq!(
        new_name(Some(name("ci")), Some(name("work")), taken),
        Some(name("ci"))
    );
    assert_eq!(new_name(None, None, taken), Some(name("default-2")));
}
