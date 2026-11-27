use super::*;

const CORE: &str = "core:0000abcd-7431";

#[tokio::test]
async fn a_herdr_2_view_scrolls_within_the_history_it_last_read() {
    let dir = tempfile::tempdir().unwrap();
    // No core answers, so no program takes the wheel: the view scrolls.
    let state = crate::web::tests::state(dir.path());
    let mut sizing = Sizing::default();
    // Before any read there is no history to scroll into.
    assert!(sizing.scroll(&state, CORE, 5).await);
    assert_eq!(sizing.up, 0);
    sizing.history = 20;
    assert!(sizing.scroll(&state, CORE, 5).await);
    assert!(sizing.scroll(&state, CORE, 50).await);
    assert_eq!(sizing.up, 20);
    assert!(sizing.scroll(&state, CORE, -8).await);
    assert_eq!(sizing.up, 12);
    assert!(sizing.scroll(&state, CORE, -100).await);
    assert_eq!(sizing.up, 0);
}

#[tokio::test]
async fn a_herdr_1_view_scrolls_only_through_a_held_attach() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let mut sizing = Sizing::default();
    assert!(!sizing.scroll(&state, "p2", 5).await);
    assert_eq!(sizing.up, 0);
}

#[test]
fn a_herdr_2_program_taking_the_wheel_returns_the_view_to_the_live_screen() {
    let mut sizing = Sizing {
        history: 20,
        ..Sizing::default()
    };
    sizing.scroll_core(8, false);
    assert_eq!(sizing.up, 8);
    sizing.scroll_core(3, true);
    assert_eq!(sizing.up, 0);
}
