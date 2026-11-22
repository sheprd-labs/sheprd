//! The core's shared workspaces (`sheprd.spaces`): read over Sheprd's
//! saved tabs once written, tab targets by document id, and a closed
//! Sheprd's changes written to the document rather than the file.

use super::*;
use crate::control::layout::Op;
use sheprd_session::spaces::{DOC, Spaces};
use std::path::Path;

pub(super) const SCOPE: &str = "local:/test.sock";

/// Sheprd's saved tabs: workspace `space-1` with tab 4 holding `t1`.
pub(super) const SAVED: &str = r#"{"workspaces": [
    {"id": "space-1", "scope": "local:/test.sock", "label": "file", "cwd": "/code/file"}],
  "tabs": [{"id": 4, "scope": "local:/test.sock", "workspace_id": "space-1", "title": "old",
    "location": {"kind": "core_terminal", "panes": "t1"}}]}"#;

/// A document holding workspace `space-a` in `/code/app`, its tab `tab-a`
/// holding `t1`.
pub(super) fn document() -> Value {
    json!({"format": 1,
        "workspaces": [{"id": "space-a", "label": "app", "custom": false, "cwd": "/code/app"}],
        "tabs": [{"id": "tab-a", "workspace_id": "space-a", "title": "build", "panes": "t1"}]})
}

/// A fake core with `records`, the saved tabs [`SAVED`] in `dir`, and the
/// shared document `doc` when one was written.
pub(super) fn fake_sharing(
    dir: &Path,
    doc: Option<Value>,
    records: Vec<Value>,
) -> (Arc<Source>, Fake) {
    let path = dir.join("browser-tabs.json");
    std::fs::write(&path, SAVED).unwrap();
    let (mut source, fake) = fake_with_doc(false, Value::Null, records);
    let editable = Arc::get_mut(&mut source).unwrap();
    editable.tabs = Some(path);
    editable.scope = Some(SCOPE.into());
    if let Some(doc) = doc {
        lock(&fake.docs).insert(DOC.to_owned(), (doc, 1));
    }
    (source, fake)
}

/// The shared document as the fake core holds it.
pub(super) fn shared(fake: &Fake) -> Spaces {
    let doc = lock(&fake.docs).get(DOC).cloned().unwrap().0;
    Spaces::from_value(doc).unwrap()
}

fn sent(fake: &Fake) -> Value {
    let bytes = fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn the_written_document_lists_kept_workspaces_and_tabs_over_the_saved_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    let layout = source.poll().unwrap();
    let kept = layout.workspace("space-a").unwrap();
    assert_eq!(
        (kept.label.as_str(), kept.cwd.as_str()),
        ("app", "/code/app")
    );
    assert_eq!(kept.terminals[0].tab, "core:tab:tab-a");
    assert_eq!(kept.terminals[0].label, "build");
    assert!(
        layout.workspace("space-1").is_none(),
        "the file stands aside"
    );
    assert!(source.keeps_workspace("space-a"));
    assert!(!source.keeps_workspace("space-1"));
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_never_written_document_leaves_the_saved_tabs_in_charge() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), None, vec![record("t1")]);
    let layout = source.poll().unwrap();
    assert_eq!(
        layout.workspace("space-1").unwrap().terminals[0].tab,
        "core:tab:4"
    );
    assert!(source.keeps_workspace("space-1"));
    assert_eq!(
        source.saved_target("core:tab:4", &id("t1")),
        Some((TabRef::File(4), "t1".to_owned()))
    );
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn tab_targets_name_document_tabs_and_numeric_ids_only_while_the_file_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    assert_eq!(
        source.saved_target("core:tab:tab-a", &id("t1")),
        Some((TabRef::Doc("tab-a".into()), "t1".to_owned()))
    );
    assert_eq!(source.saved_target("core:tab:4", &id("t1")), None);
    assert_eq!(source.saved_target("core:tab:tab-z", &id("t1")), None);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_closed_sheprds_split_zoom_names_and_close_are_written_to_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    let tab = TabRef::Doc("tab-a".into());
    let split = source.arrange_saved(&tab, "t1", &Op::SplitRight).unwrap();
    assert_eq!(split.as_deref(), Some("t9"));
    let run = sent(&fake);
    assert_eq!(run["method"], "terminal.run");
    assert_eq!(run["params"]["labels"]["sheprd.workspace"], "space-a");
    let terminals = |spaces: &Spaces| -> Vec<String> {
        let tab = spaces.tab("tab-a").unwrap();
        tab.panes
            .terminals()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    };
    assert_eq!(terminals(&shared(&fake)), ["t1", "t9"]);

    source.arrange_saved(&tab, "t1", &Op::Zoom).unwrap();
    source
        .arrange_saved(&tab, "t1", &Op::RenameTab(Some(" logs ".into())))
        .unwrap();
    source
        .arrange_saved(&tab, "t1", &Op::RenamePane(Some("server".into())))
        .unwrap();
    let spaces = shared(&fake);
    let kept = spaces.tab("tab-a").unwrap();
    assert_eq!(kept.zoomed.as_ref().map(|id| id.as_str()), Some("t1"));
    assert_eq!(kept.name.as_deref(), Some("logs"));
    assert_eq!(kept.names.get("t1").map(String::as_str), Some("server"));
    source.arrange_saved(&tab, "t1", &Op::Zoom).unwrap();
    source
        .arrange_saved(&tab, "t1", &Op::RenameTab(None))
        .unwrap();
    let spaces = shared(&fake);
    assert_eq!(spaces.tab("tab-a").unwrap().zoomed, None);
    assert_eq!(spaces.tab("tab-a").unwrap().name, None);

    source.arrange_saved(&tab, "t1", &Op::Close).unwrap();
    assert!(shared(&fake).tab("tab-a").is_none());
    assert!(shared(&fake).workspace("space-a").is_some());
    let file = std::fs::read_to_string(dir.path().join("browser-tabs.json")).unwrap();
    assert_eq!(file, SAVED, "the saved tabs are Sheprd's alone");
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_change_another_client_wrote_first_is_kept_beside_the_phones() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    let mut renamed = document();
    renamed["workspaces"][0]["label"] = json!("renamed elsewhere");
    *lock(&fake.first) = Some(renamed);
    source
        .arrange_saved(&TabRef::Doc("tab-a".into()), "t1", &Op::Zoom)
        .unwrap();
    let spaces = shared(&fake);
    assert_eq!(
        spaces.workspace("space-a").unwrap().label,
        "renamed elsewhere"
    );
    assert!(spaces.tab("tab-a").unwrap().zoomed.is_some());
    assert_eq!(
        lock(&fake.docs)[DOC].1,
        3,
        "written over the other client's"
    );
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_change_to_a_tab_gone_from_the_document_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    let tab = TabRef::Doc("tab-gone".into());
    assert!(source.arrange_saved(&tab, "t1", &Op::Zoom).is_err());
    // A pane the tab does not hold is refused too.
    let tab = TabRef::Doc("tab-a".into());
    assert!(source.arrange_saved(&tab, "t404", &Op::Zoom).is_err());
    assert_eq!(lock(&fake.docs)[DOC], (document(), 1));
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_closed_sheprds_workspace_rename_and_close_are_written_to_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let mut labelled = record("t2");
    labelled["labels"] = json!({"sheprd.workspace": "space-a"});
    let records = vec![record("t1"), labelled, record("t3")];
    let (source, fake) = fake_sharing(dir.path(), Some(document()), records);
    source.poll().unwrap();
    let rename = Op::RenameWorkspace {
        workspace: "space-a".into(),
        name: " named ".into(),
    };
    source.arrange_saved(&TabRef::File(0), "", &rename).unwrap();
    let kept = shared(&fake).workspace("space-a").cloned().unwrap();
    assert_eq!((kept.label.as_str(), kept.custom), ("named", true));

    let close = Op::CloseWorkspace {
        workspace: "space-a".into(),
    };
    source.arrange_saved(&TabRef::File(0), "", &close).unwrap();
    let mut killed: Vec<Value> = (0..2)
        .map(|_| sent(&fake)["params"]["terminal"].clone())
        .collect();
    killed.sort_by_key(ToString::to_string);
    assert_eq!(
        killed,
        [json!("t1"), json!("t2")],
        "its tab's and its labelled"
    );
    assert!(
        fake.inputs
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
    let spaces = shared(&fake);
    assert!(spaces.workspace("space-a").is_none());
    assert!(spaces.tabs().is_empty());
    drop(source);
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_running_sheprd_is_asked_by_the_saved_tab_holding_the_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc["tabs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "tab-b", "workspace_id": "space-a", "title": "b", "panes": "t2"}));
    let (source, fake) = fake_sharing(dir.path(), Some(doc), vec![record("t1"), record("t2")]);
    source.poll().unwrap();
    let mut hub = Hub::offline_in(Layout::default(), None, None);
    hub.core = Some(source.clone());
    assert_eq!(hub.desktop_tab(&TabRef::Doc("tab-a".into()), "t1"), Some(4));
    assert_eq!(hub.desktop_tab(&TabRef::Doc("tab-b".into()), "t2"), None);
    assert_eq!(hub.desktop_tab(&TabRef::File(0), ""), Some(0));
    // A tab Sheprd shows no saved tab for is changed in the document.
    hub.arrange_doc(TabRef::Doc("tab-b".into()), "t2".into(), Op::Zoom)
        .await
        .unwrap();
    assert!(shared(&fake).tab("tab-b").unwrap().zoomed.is_some());
    drop((hub, source));
    fake.server.join().unwrap();
}
