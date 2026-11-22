use super::*;
use crate::control::read::Reply;

#[test]
fn without_a_file_marks_start_now() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(load(&file(dir.path()), 42), Marks::starting(42));
}

#[test]
fn saved_marks_load_back() {
    let dir = tempfile::tempdir().unwrap();
    let file = file(dir.path());
    let mut marks = Marks::starting(1);
    marks.read(
        "p",
        Reply {
            finish: Some(5),
            news: true,
        },
        9,
    );
    save(&file, &marks).unwrap();
    assert_eq!(load(&file, 100), marks);
}

#[test]
fn a_damaged_file_costs_only_what_was_unread() {
    let dir = tempfile::tempdir().unwrap();
    let file = file(dir.path());
    fs::write(&file, b"{not json").unwrap();
    assert_eq!(load(&file, 7), Marks::starting(7));
}

#[test]
fn with_no_daemon_nothing_is_unread() {
    let dir = tempfile::tempdir().unwrap();
    let change = Change {
        panes: vec!["p".to_owned()],
        read: true,
    };
    assert_eq!(exchange(dir.path(), Some(change)).unwrap(), None);
    assert!(!file(dir.path()).exists(), "nothing saved without a daemon");
}
