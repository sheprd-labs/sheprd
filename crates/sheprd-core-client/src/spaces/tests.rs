use super::*;
use sheprd_session::spaces::Workspace;
use std::cell::RefCell;

fn workspace(id: &str) -> Workspace {
    Workspace {
        id: id.to_owned(),
        label: id.to_owned(),
        custom: false,
        cwd: String::new(),
    }
}

fn keep(id: &str) -> Edit {
    Edit::KeepWorkspace(workspace(id))
}

/// A document store that refuses a stale version, as `doc.set` does, and
/// lets `other` write once before the first write lands.
struct Store {
    doc: RefCell<Doc>,
    other: RefCell<Option<Edit>>,
    writes: RefCell<usize>,
}

impl Store {
    fn new(other: Option<Edit>) -> Self {
        Self {
            doc: RefCell::new(Doc {
                value: Value::Null,
                version: 0,
            }),
            other: RefCell::new(other),
            writes: RefCell::new(0),
        }
    }

    fn get(&self) -> Result<Doc> {
        Ok(self.doc.borrow().clone())
    }

    fn set(&self, value: &Value, version: u64) -> Result<u64> {
        if let Some(edit) = self.other.borrow_mut().take() {
            let mut spaces = Spaces::from_value(self.doc.borrow().value.clone()).unwrap();
            spaces.apply(edit).unwrap();
            let mut doc = self.doc.borrow_mut();
            doc.value = spaces.to_value().unwrap();
            doc.version += 1;
        }
        let mut doc = self.doc.borrow_mut();
        if doc.version != version {
            return Err(Error::Refused {
                method: Method::DocSet,
                kind: Some("conflict".to_owned()),
                message: "stale".to_owned(),
            });
        }
        *self.writes.borrow_mut() += 1;
        doc.value = value.clone();
        doc.version += 1;
        Ok(doc.version)
    }

    fn update(&self, edits: &[Edit]) -> Result<(Spaces, u64)> {
        update_with(
            || self.get(),
            |value, version| self.set(value, version),
            |spaces| {
                for edit in edits {
                    spaces.apply(edit.clone())?;
                }
                Ok(())
            },
        )
    }
}

fn ids(spaces: &Spaces) -> Vec<&str> {
    spaces.workspaces().iter().map(|w| w.id.as_str()).collect()
}

#[test]
fn an_edit_is_written_over_the_version_it_was_made_to() {
    let store = Store::new(None);
    let (spaces, version) = store.update(&[keep("a")]).unwrap();
    assert_eq!(ids(&spaces), ["a"]);
    assert_eq!(version, 1);
}

#[test]
fn another_clients_write_first_keeps_both_changes() {
    let store = Store::new(Some(keep("theirs")));
    let (spaces, version) = store.update(&[keep("mine")]).unwrap();
    assert_eq!(ids(&spaces), ["theirs", "mine"]);
    assert_eq!(version, 2);
    let stored = Spaces::from_value(store.doc.borrow().value.clone()).unwrap();
    assert_eq!(ids(&stored), ["theirs", "mine"]);
}

#[test]
fn no_change_writes_nothing() {
    let store = Store::new(None);
    store.update(&[keep("a")]).unwrap();
    let forget = Edit::ForgetWorkspace {
        id: "gone".to_owned(),
    };
    let (_, version) = store.update(&[forget]).unwrap();
    assert_eq!(version, 1);
    assert_eq!(*store.writes.borrow(), 1);
}

#[test]
fn a_refused_edit_leaves_the_document_as_it_was() {
    let store = Store::new(None);
    store.update(&[keep("a")]).unwrap();
    let error = store.update(&[keep("b"), keep("a")]).unwrap_err();
    assert!(matches!(
        error,
        Error::Spaces(sheprd_session::spaces::Error::Taken(_))
    ));
    let stored = Spaces::from_value(store.doc.borrow().value.clone()).unwrap();
    assert_eq!(ids(&stored), ["a"]);
}
