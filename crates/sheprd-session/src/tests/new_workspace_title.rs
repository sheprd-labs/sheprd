use crate::{NEW_WORKSPACE_LABEL, NEW_WORKSPACE_TITLE_LABEL, new_workspace_title};
use std::collections::BTreeMap;

fn labels(title: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (NEW_WORKSPACE_LABEL.to_owned(), "/code/project".to_owned()),
        (NEW_WORKSPACE_TITLE_LABEL.to_owned(), title.to_owned()),
    ])
}

#[test]
fn a_new_workspace_takes_the_title_it_was_given_trimmed() {
    assert_eq!(
        new_workspace_title(&labels("  iOS vibration ")),
        Some("iOS vibration")
    );
}

#[test]
fn a_blank_missing_or_overlong_title_names_no_workspace() {
    assert_eq!(new_workspace_title(&labels("   ")), None);
    assert_eq!(new_workspace_title(&labels(&"x".repeat(201))), None);
    let mut untitled = labels("x");
    untitled.remove(NEW_WORKSPACE_TITLE_LABEL);
    assert_eq!(new_workspace_title(&untitled), None);
}
