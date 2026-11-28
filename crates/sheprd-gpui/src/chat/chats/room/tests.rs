use super::*;

#[test]
fn the_panes_seen_last_come_first() {
    let mut seen = Seen::default();
    seen.note(["p1"]);
    seen.note(["p2", "p3", "p2"]);
    assert_eq!(seen.recency("p2"), Some(0));
    assert_eq!(seen.recency("p3"), Some(1));
    assert_eq!(seen.recency("p1"), Some(2));
    seen.note(["p1"]);
    assert_eq!(seen.recency("p1"), Some(0));
    assert_eq!(seen.recency("p2"), Some(1));
    assert_eq!(seen.recency("p9"), None);
}

#[test]
fn seen_panes_stay_bounded() {
    let mut seen = Seen::default();
    for n in 0..=MAX_SEEN {
        seen.note([format!("p{n}").as_str()]);
    }
    assert_eq!(seen.recency(&format!("p{MAX_SEEN}")), Some(0));
    assert_eq!(seen.recency("p0"), None);
}

#[test]
fn a_chat_just_left_ranks_above_one_never_seen() {
    let mut seen = Seen::default();
    seen.note(["left"]);
    seen.note(["now"]);
    let on_screen = ["now".to_owned()];
    let holding = ["files".to_owned()];
    let rank = |pane| seen.rank(pane, Some("now"), &on_screen, &holding);
    assert_eq!(rank("now"), 0);
    assert_eq!(rank("files"), 2);
    assert_eq!(rank("left"), 4);
    assert_eq!(rank("never"), u8::MAX);
    assert_eq!(seen.rank("now", None, &[], &[]), 3);
}

#[test]
fn a_resting_chat_starts_its_visual_workers_afresh() {
    let target = Target {
        endpoint: "0123456789abcdef0123456789abcdef".parse().unwrap(),
        existing: None,
        pane_id: "p1".into(),
        route: crate::chat::Route::Local,
        surface: crate::chat::Surface::Herdr,
    };
    let mut pane = ChatPane::new(target);
    let before = pane.prepared.visuals.generation.clone();
    pane.rest();
    // A job of the old generation finishing later finds no match.
    assert!(!std::rc::Rc::ptr_eq(
        &before,
        &pane.prepared.visuals.generation
    ));
}
