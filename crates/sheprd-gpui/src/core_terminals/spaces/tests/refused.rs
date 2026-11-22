use super::*;

#[test]
fn a_newer_format_is_read_and_never_written() {
    let mut core = FakeCore {
        value: serde_json::json!({
            "format": sheprd_session::spaces::FORMAT + 1,
            "workspaces": [{"id": "space-1", "label": "api", "cwd": "/src/api", "later": true}],
            "tabs": [],
            "later": {"kept": true},
        }),
        version: 7,
        writes: 0,
    };
    let mut desk = Client::new();
    desk.keep("mine", "/src/mine");
    desk.settle(&mut core);
    assert_eq!(core.writes, 0);
    assert_eq!(desk.labels(), ["api", "mine"], "nothing kept here is lost");
    assert!(
        desk.sync.take_refused().is_some(),
        "what is kept here cannot be shared"
    );

    // A change here is refused once, and the document stays as it was.
    assert!(desk.store.rename_kept(&scope(), "space-1", "backend"));
    desk.settle(&mut core);
    assert!(desk.sync.take_refused().is_some());
    assert!(desk.sync.take_refused().is_none(), "shown once");
    assert_eq!(core.writes, 0);
    assert_eq!(core.value["later"]["kept"], true);
}

#[test]
fn a_refused_edit_leaves_the_document_and_takes_it_back() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    // The laptop forgets the workspace while the desk opens a tab there.
    laptop.store.forget_kept(&scope(), &api);
    laptop.settle(&mut core);
    desk.open(&api, "t1");
    let job = desk.send().unwrap();
    assert!(matches!(&job, Job::Update(edits) if matches!(edits[..], [Edit::OpenTab(_)])));
    let refused = core.run(job);
    assert!(matches!(
        refused,
        Err(sheprd_core_client::Error::Spaces(
            sheprd_session::spaces::Error::Missing(_)
        ))
    ));
    desk.finish(refused);
    assert!(desk.sync.take_refused().is_some());
    desk.settle(&mut core);
    assert!(
        desk.labels().is_empty(),
        "the document's state is taken back"
    );
    assert!(core.spaces().tabs().is_empty());
}

#[test]
fn a_core_that_does_not_answer_gets_the_change_later() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.settle(&mut core);
    assert!(desk.store.rename_kept(&scope(), &api, "backend"));
    let job = desk.send().unwrap();
    assert!(matches!(job, Job::Update(_)));
    desk.finish(Err(sheprd_core_client::Error::Lost));
    assert!(desk.sync.take_refused().is_none());
    desk.settle(&mut core);
    assert_eq!(core.spaces().workspace(&api).unwrap().label, "backend");
}

#[test]
fn an_edit_whose_worker_never_started_is_sent_later() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.settle(&mut core);
    assert!(desk.store.rename_kept(&scope(), &api, "backend"));
    assert!(matches!(desk.send(), Some(Job::Update(_))));
    desk.sync.unsent(desk.now);
    desk.settle(&mut core);
    assert_eq!(core.spaces().workspace(&api).unwrap().label, "backend");
}

#[test]
fn a_refusal_takes_back_only_what_the_refused_edits_changed() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let web = desk.keep("web", "/src/web");
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    laptop.store.forget_kept(&scope(), &api);
    laptop.settle(&mut core);
    desk.open(&api, "t1");
    let job = desk.send().unwrap();
    let refused = core.run(job);
    desk.finish(refused);
    assert!(desk.sync.take_refused().is_some());
    // Changed here after the refused edits were sent: kept and written.
    assert!(desk.store.rename_kept(&scope(), &web, "site"));
    desk.open(&web, "t2");
    desk.settle(&mut core);
    assert_eq!(desk.labels(), ["site"]);
    assert_eq!(desk.tabs(), [tab("site", &["t2"])]);
    assert_eq!(core.spaces().workspace(&web).unwrap().label, "site");
    assert_eq!(core.spaces().tabs().len(), 1);
}
