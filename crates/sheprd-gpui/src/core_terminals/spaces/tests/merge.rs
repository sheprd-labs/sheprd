use super::*;

#[test]
fn a_new_title_here_does_not_hold_back_changes_from_elsewhere() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let shown = desk.open(&api, "t1");
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    desk.store.visited(shown, None, Some("vim"));
    assert!(laptop.store.rename_kept(&scope(), &api, "backend"));
    laptop.settle(&mut core);
    desk.settle(&mut core);
    assert_eq!(desk.labels(), ["backend"]);
    assert_eq!(desk.store.get(shown).unwrap().title, "vim");
}

#[test]
fn changes_on_both_sides_merge_record_by_record() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let web = desk.keep("web", "/src/web");
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    let read = desk.send().unwrap();
    assert_eq!(read, Job::Read);
    assert!(laptop.store.rename_kept(&scope(), &api, "backend"));
    laptop.settle(&mut core);
    // Changed here while the read was out: kept, and written after.
    assert!(desk.store.rename_kept(&scope(), &web, "site"));
    let answered = core.run(read);
    desk.finish(answered);
    assert_eq!(desk.labels(), ["backend", "site"]);
    desk.settle(&mut core);
    laptop.settle(&mut core);
    assert_eq!(laptop.labels(), ["backend", "site"]);
}

#[test]
fn a_full_store_keeps_every_tab_it_had() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let shown = desk.open(&api, "t1");
    desk.settle(&mut core);
    // Another computer keeps a workspace first in the document.
    let mut laptop = Client::new();
    laptop.settle(&mut core);
    let early = laptop.keep("early", "/src/early");
    laptop.settle(&mut core);
    let mut spaces = core.spaces();
    spaces
        .apply(Edit::MoveWorkspace {
            id: early.clone(),
            before: Some(api.clone()),
        })
        .unwrap();
    core.value = spaces.to_value().unwrap();
    core.version += 1;
    // This app has room for one more workspace of this host only.
    let elsewhere = Scope::endpoint("ssh:other");
    while desk.store.kept_room() > 1 {
        desk.store
            .keep_workspace(&elsewhere, "other", "", false)
            .unwrap();
    }
    let page = desk.store.open(scope(), &api, None, None).unwrap();
    desk.store
        .keep_workspace(&elsewhere, "full", "", false)
        .unwrap();
    desk.settle(&mut core);
    assert_eq!(desk.labels(), ["api"], "no room for the new one");
    assert!(desk.store.get(shown).is_some());
    assert!(desk.store.get(page).is_some());
    assert!(
        core.spaces().workspace(&early).is_some(),
        "nor is it forgotten"
    );
}

#[test]
fn a_zoom_here_and_a_rename_elsewhere_of_one_tab_both_stay() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let shown = desk.open(&api, "t1");
    let mut panes = Panes::single(id("t1"));
    panes
        .split(&id("t1"), crate::core_terminals::panes::Axis::Row, id("t2"))
        .unwrap();
    desk.store.visited(
        shown,
        Some(Location::core_terminal(panes.clone(), false)),
        None,
    );
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    let read = desk.send().unwrap();
    assert_eq!(read, Job::Read);
    // The laptop names the tab while the desk zooms a pane.
    let theirs = laptop.store.in_scope(&scope()).next().unwrap().id;
    let mut named = Location::core_terminal(panes.clone(), false);
    if let Location::CoreTerminal { name, .. } = &mut named {
        *name = Some("api".into());
    }
    laptop.store.visited(theirs, Some(named), None);
    laptop.settle(&mut core);
    let mut zoomed = Location::core_terminal(panes, false);
    if let Location::CoreTerminal { zoomed: zoom, .. } = &mut zoomed {
        *zoom = Some(id("t2"));
    }
    desk.store.visited(shown, Some(zoomed), None);
    let answered = core.run(read);
    desk.finish(answered);
    desk.settle(&mut core);
    laptop.settle(&mut core);

    let tab = core.spaces().tabs()[0].clone();
    assert_eq!(tab.name.as_deref(), Some("api"), "the rename stays");
    assert_eq!(tab.zoomed, Some(id("t2")), "and so does the zoom");
    let Some(Location::CoreTerminal { name, zoomed, .. }) =
        &desk.store.get(shown).unwrap().location
    else {
        panic!("a terminal tab");
    };
    assert_eq!(name.as_deref(), Some("api"));
    assert_eq!(*zoomed, Some(id("t2")));
}

#[test]
fn a_resize_over_another_clients_split_is_read_again_not_refused() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let shown = desk.open(&api, "t1");
    let mut panes = Panes::single(id("t1"));
    panes
        .split(&id("t1"), crate::core_terminals::panes::Axis::Row, id("t2"))
        .unwrap();
    desk.store.visited(
        shown,
        Some(Location::core_terminal(panes.clone(), false)),
        None,
    );
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    // The desk swaps its panes while the laptop opens a third.
    let mut swapped = panes.clone();
    assert!(swapped.swap(&id("t1"), &id("t2")));
    desk.store
        .visited(shown, Some(Location::core_terminal(swapped, false)), None);
    let resize = desk.send().unwrap();
    let theirs = laptop.store.in_scope(&scope()).next().unwrap().id;
    let mut split = panes.clone();
    split
        .split(
            &id("t2"),
            crate::core_terminals::panes::Axis::Column,
            id("t3"),
        )
        .unwrap();
    laptop
        .store
        .visited(theirs, Some(Location::core_terminal(split, false)), None);
    laptop.settle(&mut core);
    let stale = core.run(resize);
    assert!(matches!(
        stale,
        Err(sheprd_core_client::Error::Spaces(
            sheprd_session::spaces::Error::Stale(_)
        ))
    ));
    desk.finish(stale);
    assert!(desk.sync.take_refused().is_none(), "no warning for a race");
    let read = desk.send();
    assert_eq!(read, Some(Job::Read), "read again first");
    let answered = core.run(read.unwrap());
    desk.finish(answered);
    desk.settle(&mut core);
    let terminals = |panes: &Panes| {
        let mut held: Vec<String> = (panes.terminals().iter())
            .map(|t| t.as_str().to_owned())
            .collect();
        held.sort();
        held
    };
    let document = core.spaces().tabs()[0].panes.clone();
    assert_eq!(terminals(&document), ["t1", "t2", "t3"], "the split stays");
    assert_eq!(desk.tabs(), [tab("api", &["t1", "t2", "t3"])]);
}

#[test]
fn a_resize_redone_after_a_read_lands_when_the_panes_still_match() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    let shown = desk.open(&api, "t1");
    let mut panes = Panes::single(id("t1"));
    panes
        .split(&id("t1"), crate::core_terminals::panes::Axis::Row, id("t2"))
        .unwrap();
    desk.store.visited(
        shown,
        Some(Location::core_terminal(panes.clone(), false)),
        None,
    );
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);

    // The laptop splits and closes again, so the terminals match once more
    // but the desk's swap was read before.
    let mut swapped = panes.clone();
    assert!(swapped.swap(&id("t1"), &id("t2")));
    desk.store.visited(
        shown,
        Some(Location::core_terminal(swapped.clone(), false)),
        None,
    );
    let resize = desk.send().unwrap();
    let theirs = laptop.store.in_scope(&scope()).next().unwrap().id;
    laptop.store.visited(
        theirs,
        Some(Location::core_terminal(Panes::single(id("t1")), false)),
        None,
    );
    laptop.settle(&mut core);
    laptop
        .store
        .visited(theirs, Some(Location::core_terminal(panes, false)), None);
    laptop.settle(&mut core);
    let answered = core.run(resize);
    desk.finish(answered);
    desk.settle(&mut core);
    assert!(desk.sync.take_refused().is_none());
    assert_eq!(
        core.spaces().tabs()[0].panes,
        swapped,
        "the swap was redone"
    );
}

#[test]
fn a_record_with_no_room_is_taken_once_room_is_made() {
    let mut core = FakeCore::default();
    let mut desk = Client::new();
    let api = desk.keep("api", "/src/api");
    desk.settle(&mut core);
    let mut laptop = Client::new();
    laptop.settle(&mut core);
    laptop.open(&api, "t1");
    laptop.settle(&mut core);

    // The desk has no room for another tab when it reads the laptop's.
    let mut pages = Vec::new();
    while desk.store.room() > 0 {
        pages.push(desk.store.open(scope(), &api, None, None).unwrap());
    }
    desk.settle(&mut core);
    assert!(desk.tabs().is_empty());
    let version = core.version;
    desk.store.close(pages[0]);
    desk.settle(&mut core);
    assert_eq!(core.version, version, "the document did not change");
    assert_eq!(desk.tabs(), [tab("api", &["t1"])]);
}
