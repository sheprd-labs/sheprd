use super::*;

fn replied(ms: u64) -> Reply {
    Reply {
        finish: Some(ms),
        news: true,
    }
}

const QUIET: Reply = Reply {
    finish: None,
    news: false,
};

#[test]
fn replies_before_marks_were_kept_count_as_read() {
    let marks = Marks::starting(100);
    assert!(!marks.unread("p", replied(100)));
    assert!(marks.unread("p", replied(101)));
    assert!(!marks.unread("p", QUIET));
    // Found done when the daemon started, at no known time: it came before.
    let untimed = Reply {
        finish: None,
        news: true,
    };
    assert!(!marks.unread("p", untimed));
}

#[test]
fn reading_lasts_until_the_next_reply() {
    let mut marks = Marks::starting(0);
    assert!(marks.unread("p", replied(5)));
    assert!(marks.read("p", replied(5), 10));
    assert!(!marks.unread("p", replied(5)));
    assert!(!marks.read("p", replied(5), 11), "nothing new to read");
    assert!(marks.unread("p", replied(6)));
}

#[test]
fn reading_a_pane_with_nothing_to_read_keeps_no_mark() {
    let mut marks = Marks::starting(0);
    assert!(!marks.read("p", QUIET, 1));
    assert!(marks.panes.is_empty());
    assert!(marks.unread("p", replied(2)), "its first reply shows");
}

#[test]
fn a_pane_marked_unread_stays_so_until_read() {
    let mut marks = Marks::starting(0);
    marks.read("p", replied(5), 10);
    assert!(marks.mark_unread("p", replied(5), 11));
    assert!(!marks.mark_unread("p", replied(5), 12));
    assert!(marks.unread("p", replied(5)));
    assert!(marks.read("p", replied(5), 13));
    assert!(!marks.unread("p", replied(5)));
}

#[test]
fn marking_unread_a_pane_with_nothing_to_read_still_shows() {
    let mut marks = Marks::starting(0);
    assert!(marks.mark_unread("p", QUIET, 1));
    assert!(marks.unread("p", QUIET));
    assert!(marks.read("p", QUIET, 2));
    assert!(!marks.unread("p", QUIET));
}

#[test]
fn reading_an_unmarked_old_reply_after_marking_it_unread_lands_where_it_was() {
    let mut marks = Marks::starting(100);
    marks.mark_unread("p", replied(50), 120);
    marks.read("p", QUIET, 130);
    assert!(!marks.unread("p", replied(50)));
}

#[test]
fn bounds_drop_invalid_panes_and_keep_the_newest() {
    let mut marks = Marks::starting(0);
    marks.panes.insert(" ".to_owned(), Mark::default());
    for at in 0..(MAX_MARKS as u64 + 5) {
        let mark = Mark {
            seen: Some(at),
            unread: false,
            at_ms: at,
        };
        marks.panes.insert(format!("p{at}"), mark);
    }
    let marks = marks.bounded();
    assert_eq!(marks.panes.len(), MAX_MARKS);
    assert!(!marks.panes.contains_key(" "));
    assert!(!marks.panes.contains_key("p0"));
    assert!(marks.panes.contains_key(&format!("p{}", MAX_MARKS + 4)));
}

#[test]
fn a_new_herdr_boot_drops_its_panes_and_reads_what_came_before() {
    let mut marks = Marks::starting(0);
    assert!(marks.follow_boot("b1", 1));
    marks.read("3", replied(5), 2);
    marks.read("core:00000000-61", replied(5), 2);
    assert!(!marks.follow_boot("b1", 3));
    assert!(marks.follow_boot("b2", 50));
    assert!(!marks.panes.contains_key("3"));
    assert!(marks.panes.contains_key("core:00000000-61"));
    assert!(
        !marks.unread("3", replied(40)),
        "restored before the restart"
    );
    assert!(marks.unread("3", replied(51)));
}

#[test]
fn changes_are_bounded() {
    let change = |panes: Vec<String>| Change { panes, read: true };
    assert!(change(vec!["p".to_owned()]).valid());
    assert!(!change(Vec::new()).valid());
    assert!(!change(vec!["\n".to_owned()]).valid());
    assert!(!change(vec!["p".to_owned(); MAX_CHANGE + 1]).valid());
}

#[test]
fn marks_keep_their_wire_names() {
    let mut marks = Marks::starting(7);
    marks.read("p", replied(5), 9);
    assert_eq!(
        serde_json::to_value(&marks).unwrap(),
        serde_json::json!({"since_ms": 7, "panes": {"p": {"seen": 5, "at_ms": 9}}})
    );
    let change: Change = serde_json::from_str(r#"{"panes":["p"],"read":false}"#).unwrap();
    assert!(!change.read);
}

#[test]
fn a_read_reply_whose_time_is_lost_stays_read() {
    // The daemon restarted: a terminal still done has no known time.
    let mut marks = Marks::starting(0);
    marks.read("3", replied(5), 9);
    let untimed = Reply {
        finish: None,
        news: true,
    };
    assert!(!marks.unread("3", untimed));
    assert!(!marks.read("3", untimed, 10), "what was read is kept");
    assert!(marks.unread("3", replied(6)));
}

#[test]
fn a_herdr_restart_leaves_other_panes_cutoff_alone() {
    let mut marks = Marks::starting(0);
    marks.follow_boot("b1", 1);
    assert!(marks.follow_boot("b2", 50));
    assert!(
        marks.unread("core:00000000-61", replied(40)),
        "unread before the restart"
    );
    assert!(
        marks.unread("herdr:work:2", replied(40)),
        "a named session is not restarted"
    );
    assert!(!marks.unread("3", replied(40)));
}

#[test]
fn marks_stay_bounded_as_they_are_made() {
    let mut marks = Marks::starting(0);
    for at in 0..(MAX_MARKS as u64 + 3) {
        marks.read(&format!("p{at}"), replied(at + 1), at);
    }
    assert_eq!(marks.panes.len(), MAX_MARKS);
    assert!(!marks.panes.contains_key("p0"), "the oldest went first");
    marks.mark_unread("late", QUIET, u64::MAX);
    assert_eq!(marks.panes.len(), MAX_MARKS);
    assert!(marks.panes.contains_key("late"));
}
