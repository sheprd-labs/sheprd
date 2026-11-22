//! How the worker reads the core's shared workspaces: which of them and
//! Sheprd's saved tabs it lists, and what a slow, refused, or unreadable
//! answer leaves.

use super::*;
use crate::hub::core::poller::Poller;
use sheprd_core_client::{Error as CoreError, Method, wire::Doc};

fn shared(version: u64, label: &str) -> sheprd_core_client::Result<Doc> {
    Ok(Doc {
        value: json!({"format": 1,
            "workspaces": [{"id": "space-a", "label": label, "cwd": "/code/app"}],
            "tabs": [{"id": "tab-a", "workspace_id": "space-a", "title": "t", "panes": "t1"}]}),
        version,
    })
}

fn labels(poller: &mut Poller) -> Vec<String> {
    poller
        .project("fake-core", &[], None, None)
        .workspaces
        .into_iter()
        .map(|workspace| workspace.label)
        .collect()
}

#[test]
fn only_a_written_document_is_read_and_only_a_refusal_forgets_it() {
    let mut poller = Poller::default();
    poller
        .read_shared(Ok(Doc {
            value: Value::Null,
            version: 0,
        }))
        .unwrap();
    assert!(poller.shared().is_none());
    poller.read_shared(shared(1, "desk")).unwrap();
    assert_eq!(labels(&mut poller), ["desk"]);
    for error in [CoreError::Timeout(Method::DocGet), CoreError::Busy] {
        poller.read_shared(Err(error)).unwrap();
        assert_eq!(labels(&mut poller), ["desk"]);
    }
    poller.read_shared(shared(2, "renamed")).unwrap();
    assert_eq!(labels(&mut poller), ["renamed"]);
    let refused = CoreError::Refused {
        kind: None,
        message: "no".into(),
        method: Method::DocGet,
    };
    poller.read_shared(Err(refused)).unwrap();
    assert!(poller.shared().is_none());
    assert!(matches!(
        poller.read_shared(Err(CoreError::Lost)),
        Err(HubError::Core(Error::Client(CoreError::Lost)))
    ));
}

#[test]
fn an_unreadable_document_leaves_the_saved_tabs_in_charge() {
    let mut poller = Poller::default();
    let unreadable = Doc {
        value: json!("not a document"),
        version: 4,
    };
    poller.read_shared(Ok(unreadable)).unwrap();
    assert!(poller.shared().is_none());
}

#[test]
fn another_cores_document_is_forgotten() {
    let mut poller = Poller::default();
    poller
        .read_spaces(
            "old",
            Ok(Doc {
                value: Value::Null,
                version: 0,
            }),
        )
        .unwrap();
    poller.read_shared(shared(1, "old")).unwrap();
    poller
        .read_spaces(
            "new",
            Ok(Doc {
                value: Value::Null,
                version: 0,
            }),
        )
        .unwrap();
    assert!(poller.shared().is_none());
}

#[test]
fn a_recovered_chat_keeps_its_place_name_and_zoom() {
    let mut tab: sheprd_session::spaces::Tab = serde_json::from_value(json!({
        "id": "tab-a", "workspace_id": "space-a", "title": "t",
        "panes": {"axis": "row", "first": "t1", "second": "t2"},
        "names": {"t2": "chat"}, "zoomed": "t2"}))
    .unwrap();
    let old = sheprd_backend::TerminalId::new("t2").unwrap();
    let new = sheprd_backend::TerminalId::new("t7").unwrap();
    kept::remap(
        &mut tab,
        &std::collections::BTreeMap::from([(old, new.clone())]),
    );
    assert!(tab.panes.contains(&new));
    assert_eq!(tab.names.get("t7").map(String::as_str), Some("chat"));
    assert_eq!(tab.zoomed, Some(new));
}
