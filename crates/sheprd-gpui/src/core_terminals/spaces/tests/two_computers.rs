use super::*;

#[test]
fn another_computer_lists_and_changes_what_one_kept() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let first = desk.open(&api, "t1");
    desk.settle(&mut core);
    assert_eq!(core.spaces().workspaces().len(), 1);
    assert_eq!(core.spaces().tabs().len(), 1);

    let mut laptop = Client::new();
    laptop.settle(&mut core);
    assert_eq!(laptop.labels(), ["api"]);
    assert_eq!(laptop.tabs(), [tab("api", &["t1"])]);

    // A rename there shows here.
    assert!(laptop.store.rename_kept(&scope(), &api, "backend"));
    laptop.settle(&mut core);
    desk.settle(&mut core);
    assert_eq!(desk.labels(), ["backend"]);
    let renamed = desk.store.kept_workspace(&scope(), &api).unwrap();
    assert!(renamed.custom);

    // A split here shows there, and the pane in use stays each one's own.
    let mut panes = Panes::single(id("t1"));
    panes
        .split(&id("t1"), crate::core_terminals::panes::Axis::Row, id("t2"))
        .unwrap();
    let mut location = Location::core_terminal(panes, false);
    if let Location::CoreTerminal { active, .. } = &mut location {
        *active = Some(id("t2"));
    }
    desk.store.visited(first, Some(location), None);
    desk.settle(&mut core);
    laptop.settle(&mut core);
    assert_eq!(laptop.tabs(), [tab("backend", &["t1", "t2"])]);
    let here = scope();
    let shown = laptop.store.in_scope(&here).next().unwrap();
    assert!(matches!(
        &shown.location,
        Some(Location::CoreTerminal { active: None, .. })
    ));

    // A tab closed there goes here, ending nothing here itself.
    let theirs = laptop.store.in_scope(&scope()).next().unwrap().id;
    laptop.store.close(theirs);
    laptop.settle(&mut core);
    desk.settle(&mut core);
    assert!(desk.tabs().is_empty());
    assert!(!desk.store.has_ending());
    assert!(
        desk.store.dropped_shared(first),
        "no window here ends its terminals"
    );
    assert_eq!(core.spaces().tabs().len(), 0);
}

#[test]
fn a_quiet_document_is_only_read() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.open(&api, "t1");
    desk.settle(&mut core);
    let writes = core.writes;
    for _ in 0..3 {
        desk.settle(&mut core);
    }
    assert_eq!(core.writes, writes);
    // A title the program sets is not a change to share.
    let tab = desk.store.in_scope(&scope()).next().unwrap().id;
    desk.store.visited(tab, None, Some("vim"));
    desk.settle(&mut core);
    assert_eq!(core.writes, writes);
}

#[test]
fn two_computers_changing_at_once_both_keep_their_change() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    // From the same document: a rename on one, a new workspace on the other.
    assert!(desk.store.rename_kept(&scope(), &api, "backend"));
    laptop.keep("web", "/src/web");
    let from_desk = desk.send().unwrap();
    let from_laptop = laptop.send().unwrap();
    assert!(matches!(from_desk, Job::Update(_)));
    assert!(matches!(from_laptop, Job::Update(_)));
    // The laptop writes first; the desk's edit is made again to what it
    // wrote.
    let answered = core.run(from_laptop);
    laptop.finish(answered);
    let answered = core.run(from_desk);
    desk.finish(answered);
    desk.settle(&mut core);
    laptop.settle(&mut core);

    assert_eq!(desk.labels(), ["backend", "web"]);
    assert_eq!(laptop.labels(), ["backend", "web"]);
}

#[test]
fn a_change_made_while_a_job_is_out_is_not_overwritten() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.settle(&mut core);
    let read = desk.send().unwrap();
    assert_eq!(read, Job::Read);
    // Another client renames while this one opens a tab.
    let mut laptop = Client::new();
    laptop.settle(&mut core);
    assert!(laptop.store.rename_kept(&scope(), &api, "backend"));
    laptop.settle(&mut core);
    desk.open(&api, "t1");
    let answered = core.run(read);
    desk.finish(answered);
    assert_eq!(
        desk.tabs(),
        [tab("backend", &["t1"])],
        "the rename is taken and the new tab stays"
    );
    desk.settle(&mut core);
    assert_eq!(desk.tabs(), [tab("backend", &["t1"])]);
    assert_eq!(core.spaces().tabs().len(), 1);
}
