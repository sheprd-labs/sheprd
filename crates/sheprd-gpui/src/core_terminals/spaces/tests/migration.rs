use super::*;

#[test]
fn the_first_read_adopts_what_each_computer_kept_once() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.open(&api, "t1");
    desk.settle(&mut core);

    // The laptop kept the same folder before the document existed, with a
    // tab of the same terminal and one of its own.
    let mut laptop = Client::new();
    let mine = laptop.keep("api here", "/src/api");
    laptop.open(&mine, "t1");
    laptop.open(&mine, "t2");
    let docs = laptop.keep("docs", "/src/docs");
    laptop.settle(&mut core);

    let spaces = core.spaces();
    assert_eq!(spaces.workspaces().len(), 2, "the folder joined the desk's");
    assert_eq!(spaces.tabs().len(), 2, "t1 is in one tab only");
    assert_eq!(laptop.labels(), ["api", "docs"]);
    assert_eq!(laptop.tabs(), [tab("api", &["t1"]), tab("api", &["t2"])]);
    assert!(
        laptop.store.kept_workspace(&scope(), &api).is_some(),
        "renamed to the document's id"
    );
    assert!(laptop.store.kept_workspace(&scope(), &docs).is_some());
    assert!(
        !laptop.store.has_ending(),
        "the tab left out ends nothing: the desk's tab holds t1"
    );

    // Both restart: nothing is adopted twice.
    desk.restart();
    laptop.restart();
    desk.settle(&mut core);
    laptop.settle(&mut core);
    assert_eq!(core.spaces().workspaces().len(), 2);
    assert_eq!(core.spaces().tabs().len(), 2);
    assert_eq!(desk.labels(), ["api", "docs"]);
    assert_eq!(desk.tabs(), [tab("api", &["t1"]), tab("api", &["t2"])]);
}

#[test]
fn what_went_from_the_document_while_away_stays_gone() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.open(&api, "t1");
    let web = desk.keep("web", "/src/web");
    desk.settle(&mut core);

    // The desk quits; a phone's client forgets one workspace and the desk
    // makes nothing new meanwhile.
    let mut phone = Client::new();
    phone.settle(&mut core);
    phone.store.forget_kept(&scope(), &api);
    phone.settle(&mut core);

    desk.restart();
    // A workspace kept while away is new, and written.
    let notes = desk.keep("notes", "/src/notes");
    desk.settle(&mut core);
    assert_eq!(desk.labels(), ["web", "notes"]);
    assert!(desk.tabs().is_empty());
    let spaces = core.spaces();
    assert!(spaces.workspace(&api).is_none());
    assert!(spaces.workspace(&web).is_some());
    assert!(spaces.workspace(&notes).is_some());
}

#[test]
fn the_default_workspace_is_one_for_every_computer() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let default = sheprd_session::DEFAULT_WORKSPACE;
    assert!(desk.store.keep_default(&scope(), default, "Herdr 2"));
    desk.open(default, "t1");
    desk.settle(&mut core);
    let mut laptop = Client::new();
    assert!(laptop.store.keep_default(&scope(), default, "Herdr 2"));
    laptop.open(default, "t2");
    laptop.settle(&mut core);
    let spaces = core.spaces();
    assert_eq!(spaces.workspaces().len(), 1);
    assert_eq!(spaces.tabs_of(default).count(), 2);
}

#[test]
fn a_new_document_takes_what_the_app_holds_again() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.open(&api, "t1");
    desk.settle(&mut core);
    // The core's state was wiped: a document never written.
    let mut wiped = FakeCore::default();
    desk.settle(&mut wiped);
    assert_eq!(desk.labels(), ["api"], "nothing went with it");
    assert_eq!(desk.tabs(), [tab("api", &["t1"])]);
    assert_eq!(wiped.spaces().workspaces().len(), 1);
    assert_eq!(wiped.spaces().tabs().len(), 1);
}

#[test]
fn a_document_wiped_while_the_app_was_closed_takes_what_it_kept() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.open(&api, "t1");
    desk.settle(&mut core);
    desk.settle(&mut core);
    assert!(core.version > 0);
    desk.restart();
    let mut wiped = FakeCore::default();
    desk.settle(&mut wiped);
    assert_eq!(desk.labels(), ["api"]);
    assert_eq!(desk.tabs(), [tab("api", &["t1"])]);
    assert_eq!(wiped.spaces().workspaces().len(), 1);
    assert_eq!(wiped.spaces().tabs().len(), 1);
}

#[test]
fn a_rename_made_while_the_first_read_is_out_is_written() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let first = desk.send().unwrap();
    assert!(matches!(first, Job::Migrate(_)));
    assert!(desk.store.rename_kept(&scope(), &api, "backend"));
    let answered = core.run(first);
    desk.finish(answered);
    assert_eq!(desk.labels(), ["backend"]);
    desk.settle(&mut core);
    assert_eq!(core.spaces().workspace(&api).unwrap().label, "backend");
}

#[test]
fn adoption_waits_for_the_first_read() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    assert!(!desk.sync.answered());
    let job = desk.send().unwrap();
    assert!(!desk.sync.answered(), "not while the first read is out");
    let answered = core.run(job);
    desk.finish(answered);
    assert!(desk.sync.answered());
}
