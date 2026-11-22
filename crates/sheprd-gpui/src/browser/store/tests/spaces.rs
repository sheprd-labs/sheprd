use super::*;
use crate::browser::shared as spaces;
use crate::core_terminals::panes::Panes;
use sheprd_backend::TerminalId;

fn terminal(id: &str) -> Option<Location> {
    let id = TerminalId::new(id).unwrap();
    Some(Location::core_terminal(Panes::single(id), false))
}

#[test]
fn only_tabs_that_type_into_a_kept_workspaces_terminals_are_shared() {
    let scope = Scope::endpoint("ssh:box");
    let mut store = Store::default();
    let kept = store
        .keep_workspace(&scope, "api", "/src/api", false)
        .unwrap();
    let shared = store
        .open(scope.clone(), &kept, terminal("t1"), None)
        .unwrap();
    let mirror = Location::core_terminal(Panes::single(TerminalId::new("t1").unwrap()), true);
    store
        .open(scope.clone(), &kept, Some(mirror), None)
        .unwrap();
    store
        .open(scope.clone(), &kept, url("http://localhost:3000"), None)
        .unwrap();
    // A Herdr 1 workspace's terminal tab is not the document's.
    store
        .open(scope.clone(), "w_1", terminal("t2"), None)
        .unwrap();

    assert!(store.unnamed_shared_tabs(&scope));
    assert!(store.name_shared_tabs(&scope, || "tab-1".to_owned()));
    assert!(!store.unnamed_shared_tabs(&scope));
    assert!(!store.name_shared_tabs(&scope, || "tab-2".to_owned()));
    let view = store.shared_view(&scope);
    assert_eq!(view.workspaces.len(), 1);
    assert_eq!(view.tabs.len(), 1);
    assert_eq!(view.tabs[0].id, "tab-1");
    assert_eq!(store.get(shared).unwrap().space.as_deref(), Some("tab-1"));
    assert!(store.shared_view(&Scope::endpoint("other")).tabs.is_empty());
}

#[test]
fn a_tabs_document_id_and_what_was_synced_survive_a_restart() {
    let scope = Scope::endpoint("ssh:box");
    let mut store = Store::default();
    let kept = store
        .keep_workspace(&scope, "api", "/src/api", false)
        .unwrap();
    store
        .open(scope.clone(), &kept, terminal("t1"), None)
        .unwrap();
    store.name_shared_tabs(&scope, || "tab-1".to_owned());
    let synced = store.shared_view(&scope).synced(3);
    store.set_synced(&scope, Some(synced.clone()));
    let bytes = serde_json::to_vec(&Saved {
        synced: store.synced.clone(),
        tabs: store.tabs.clone(),
        workspaces: store.workspaces.clone(),
    })
    .unwrap();
    let read = parse(&bytes).unwrap();
    assert_eq!(read.tabs[0].space.as_deref(), Some("tab-1"));
    assert_eq!(read.synced.get(scope.as_str()), Some(&synced));
    // Files from before carry neither.
    let old = br#"{"tabs": []}"#;
    assert!(parse(old).unwrap().synced.is_empty());
}

fn id(terminal: &str) -> TerminalId {
    TerminalId::new(terminal).unwrap()
}

/// A store holding one kept workspace with one shared tab of `panes`, and
/// the document as it holds them.
fn shared(panes: Panes) -> (Store, Scope, spaces::View, TabId) {
    let scope = Scope::endpoint("ssh:box");
    let mut store = Store::default();
    let kept = store
        .keep_workspace(&scope, "api", "/src/api", false)
        .unwrap();
    let location = Some(Location::core_terminal(panes, false));
    let tab = store.open(scope.clone(), &kept, location, None).unwrap();
    store.name_shared_tabs(&scope, || "tab-1".to_owned());
    let document = store.shared_view(&scope);
    (store, scope, document, tab)
}

fn panes_of(store: &Store, tab: TabId) -> Panes {
    match &store.get(tab).unwrap().location {
        Some(Location::CoreTerminal { panes, .. }) => panes.clone(),
        _ => panic!("a terminal tab"),
    }
}

#[test]
fn a_pane_swapped_for_another_in_a_full_tab_survives_a_merge() {
    let mut full = Panes::single(id("t0"));
    for n in 1..sheprd_session::panes::MAX_PANES {
        full.split(
            &id("t0"),
            crate::core_terminals::panes::Axis::Row,
            id(&format!("t{n}")),
        )
        .unwrap();
    }
    let (mut store, scope, mut document, tab) = shared(full.clone());
    // Here one pane closes and another opens; elsewhere the tab is named.
    let mut mine = full.clone();
    mine.remove(&id("t1"));
    mine.split(
        &id("t0"),
        crate::core_terminals::panes::Axis::Row,
        id("new"),
    )
    .unwrap();
    store.visited(
        tab,
        Some(Location::core_terminal(mine.clone(), false)),
        None,
    );
    let before = document.clone();
    document.tabs[0].name = Some("api".into());
    let local = store.shared_view(&scope);
    let edits = crate::core_terminals::spaces_edits_for_test(&before, &local);
    let pending = spaces::Pending::of(&edits);
    store.mirror_shared(&scope, &document, &pending, |_| false);
    let merged = panes_of(&store, tab);
    assert!(merged.contains(&id("new")), "the new pane stays");
    assert!(!merged.contains(&id("t1")));
}

#[test]
fn a_long_shared_title_is_bounded_as_the_store_keeps_titles() {
    let (mut store, scope, mut document, _) = shared(Panes::single(id("t1")));
    let mut theirs = document.tabs[0].clone();
    theirs.id = "tab-2".into();
    theirs.panes = Panes::single(id("t2"));
    theirs.title = "x".repeat(900);
    document.tabs.push(theirs);
    store.mirror_shared(&scope, &document, &spaces::Pending::default(), |_| false);
    let added = store
        .in_scope(&scope)
        .find(|tab| tab.space.as_deref() == Some("tab-2"))
        .unwrap();
    assert!(added.title.chars().count() <= MAX_TITLE_CHARS);
    let bytes = serde_json::to_vec(&Saved {
        synced: store.synced.clone(),
        tabs: store.tabs.clone(),
        workspaces: store.workspaces.clone(),
    })
    .unwrap();
    assert!(parse(&bytes).is_ok(), "the saved tabs still read");
}

#[test]
fn a_tab_whose_panes_all_change_keeps_the_new_one_in_a_merge() {
    let mut panes = Panes::single(id("t2"));
    panes
        .split(&id("t2"), crate::core_terminals::panes::Axis::Row, id("t1"))
        .unwrap();
    let (mut store, scope, mut document, tab) = shared(panes);
    // Both panes replaced by one here; the tab named elsewhere.
    store.visited(
        tab,
        Some(Location::core_terminal(Panes::single(id("t3")), false)),
        None,
    );
    let before = document.clone();
    document.tabs[0].name = Some("api".into());
    let local = store.shared_view(&scope);
    let edits = crate::core_terminals::spaces_edits_for_test(&before, &local);
    let pending = spaces::Pending::of(&edits);
    store.mirror_shared(&scope, &document, &pending, |_| false);
    assert_eq!(panes_of(&store, tab), Panes::single(id("t3")));
    let Some(Location::CoreTerminal { name, .. }) = &store.get(tab).unwrap().location else {
        panic!("a terminal tab");
    };
    assert_eq!(name.as_deref(), Some("api"));
}
