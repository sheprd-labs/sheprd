//! Which actions each menu lists, decoded from Herdr's answers as it sends
//! them, and in which order.

use super::catalog;
use crate::plugin_actions::{
    Row, Scope,
    catalog::{Catalog, Installed, Listed, Platform},
};
use serde_json::json;

const PANE: Scope = Scope::Pane { selection: false };
const PANE_WITH_SELECTION: Scope = Scope::Pane { selection: true };

/// `plugin/action` of each row a menu for `scope` shows, in order.
fn listed(catalog: &Catalog, scope: Scope) -> Vec<String> {
    catalog
        .rows(scope)
        .iter()
        .map(|row| format!("{}/{}", row.plugin_id, row.action_id))
        .collect()
}

fn row<'a>(rows: &'a [Row], action: &str) -> &'a Row {
    rows.iter().find(|row| row.action_id == action).unwrap()
}

/// Each menu lists the actions of its own context and no others; an action
/// without a context a menu knows is in none, and one context string this
/// client does not know never costs the rest of the list.
#[test]
fn each_menu_lists_only_actions_for_its_context() {
    let catalog = catalog(
        json!([
            {"plugin_id": "hunk", "action_id": "no-contexts", "title": "No contexts"},
            {"plugin_id": "hunk", "action_id": "empty-contexts", "title": "Empty contexts", "contexts": []},
            {"plugin_id": "hunk", "action_id": "palette-only", "title": "Palette only", "contexts": ["global"]},
            {"plugin_id": "hunk", "action_id": "terminal-only", "title": "Terminal only", "contexts": ["terminal"]},
            {"plugin_id": "hunk", "action_id": "terminal-or-pane", "title": "Terminal or pane", "contexts": ["terminal", "pane"]},
            {"plugin_id": "hunk", "action_id": "workspace", "title": "Workspace", "contexts": ["workspace", "global"]},
            {"plugin_id": "hunk", "action_id": "tab", "title": "Tab", "contexts": ["tab"]},
            {"plugin_id": "hunk", "action_id": "pane", "title": "Pane", "contexts": ["pane"]},
            {"plugin_id": "hunk", "action_id": "selection", "title": "Selection", "contexts": ["selection"]},
            {"plugin_id": "hunk", "action_id": "pane-or-selection", "title": "Pane or selection", "contexts": ["pane", "selection"]},
        ]),
        json!([{"plugin_id": "hunk", "name": "Hunk", "enabled": true}]),
    );
    assert_eq!(listed(&catalog, Scope::Workspace), ["hunk/workspace"]);
    assert_eq!(listed(&catalog, Scope::Tab), ["hunk/tab"]);
    assert_eq!(
        listed(&catalog, PANE),
        [
            "hunk/pane",
            "hunk/pane-or-selection",
            "hunk/terminal-or-pane"
        ]
    );
    let rows = catalog.rows(PANE);
    assert!(rows.iter().all(|row| !row.takes_selection), "{rows:?}");

    // A selection adds the actions that take one, each still listed once.
    assert_eq!(
        listed(&catalog, PANE_WITH_SELECTION),
        [
            "hunk/pane",
            "hunk/pane-or-selection",
            "hunk/selection",
            "hunk/terminal-or-pane"
        ]
    );
    let rows = catalog.rows(PANE_WITH_SELECTION);
    assert!(row(&rows, "selection").takes_selection);
    assert!(row(&rows, "pane-or-selection").takes_selection);
    assert!(!row(&rows, "pane").takes_selection);
    assert!(!row(&rows, "terminal-or-pane").takes_selection);
}

/// `plugin.action.list` names disabled plugins' actions too, and a plugin
/// removed between the two answers is missing from `plugin.list`.
#[test]
fn disabled_and_removed_plugins_list_nothing() {
    let catalog = catalog(
        json!([
            {"plugin_id": "hunk", "action_id": "review", "title": "Review", "contexts": ["pane"]},
            {"plugin_id": "notes", "action_id": "append", "title": "Append", "contexts": ["pane"]},
            {"plugin_id": "gone", "action_id": "stale", "title": "Stale", "contexts": ["pane"]},
        ]),
        json!([
            {"plugin_id": "hunk", "name": "Hunk", "enabled": true},
            {"plugin_id": "notes", "name": "Notes", "enabled": false},
        ]),
    );
    assert_eq!(listed(&catalog, PANE), ["hunk/review"]);
}

/// The host's `uname -s` decides which platform-limited actions show; a
/// host whose platform is unknown leaves that check to Herdr. A platform
/// name this client does not know decodes, and never matches a known host.
#[test]
fn platform_limited_actions_follow_the_hosts_uname() {
    let listed_actions = json!([
        {"plugin_id": "hunk", "action_id": "anywhere", "title": "Anywhere", "contexts": ["pane"]},
        {"plugin_id": "hunk", "action_id": "linux", "title": "Linux", "contexts": ["pane"], "platforms": ["linux"]},
        {"plugin_id": "hunk", "action_id": "macos", "title": "Macos", "contexts": ["pane"], "platforms": ["macos"]},
        {"plugin_id": "hunk", "action_id": "plan9", "title": "Plan9", "contexts": ["pane"], "platforms": ["plan9"]},
        {"plugin_id": "hunk", "action_id": "unix", "title": "Unix", "contexts": ["pane"], "platforms": ["linux", "macos"]},
    ]);
    let plugins = json!([{"plugin_id": "hunk", "name": "Hunk", "enabled": true}]);
    for (uname, expected) in [
        ("Darwin\n", &["anywhere", "macos", "unix"][..]),
        ("Linux", &["anywhere", "linux", "unix"]),
        ("FreeBSD", &["anywhere", "linux", "macos", "plan9", "unix"]),
        ("", &["anywhere", "linux", "macos", "plan9", "unix"]),
    ] {
        let listed: Vec<Listed> = serde_json::from_value(listed_actions.clone()).unwrap();
        let installed: Vec<Installed> = serde_json::from_value(plugins.clone()).unwrap();
        let catalog = Catalog::new(Platform::of_uname(uname), listed, &installed);
        let shown: Vec<String> = catalog
            .rows(PANE)
            .into_iter()
            .map(|row| row.action_id)
            .collect();
        assert_eq!(shown, expected, "{uname:?}");
    }
}

/// Rows group by plugin, the groups by name (case aside) then id, the rows
/// in a group by title (case aside) then id, and only a group's first row
/// carries the heading. Two plugins that share a name stay two groups.
#[test]
fn rows_group_under_one_heading_per_plugin() {
    let catalog = catalog(
        json!([
            {"plugin_id": "notes", "action_id": "b", "title": "append", "contexts": ["pane"]},
            {"plugin_id": "hunk-nightly", "action_id": "review", "title": "Review", "contexts": ["pane"]},
            {"plugin_id": "notes", "action_id": "a", "title": "Append", "contexts": ["pane"]},
            {"plugin_id": "hunk", "action_id": "stage", "title": "stage hunk", "contexts": ["pane"]},
            {"plugin_id": "hunk", "action_id": "diff", "title": "Diff", "contexts": ["pane"]},
            {"plugin_id": "aider", "action_id": "ask", "title": "Ask", "contexts": ["pane"]},
        ]),
        json!([
            {"plugin_id": "notes", "name": "Notes", "enabled": true},
            {"plugin_id": "hunk-nightly", "name": "Hunk", "enabled": true},
            {"plugin_id": "hunk", "name": "Hunk", "enabled": true},
            {"plugin_id": "aider", "name": "aider", "enabled": true},
        ]),
    );
    let rows: Vec<(String, Option<String>)> = catalog
        .rows(PANE)
        .into_iter()
        .map(|row| (format!("{}/{}", row.plugin_id, row.action_id), row.heading))
        .collect();
    let expected = [
        ("aider/ask", Some("aider")),
        ("hunk/diff", Some("Hunk")),
        ("hunk/stage", None),
        ("hunk-nightly/review", Some("Hunk")),
        ("notes/a", Some("Notes")),
        ("notes/b", None),
    ]
    .map(|(row, heading)| (row.to_owned(), heading.map(str::to_owned)));
    assert_eq!(rows, expected);
}

/// A manifest's title or name with line breaks or other control characters
/// stays one menu line, and a blank description adds no second tooltip line.
#[test]
fn titles_stay_one_line_and_blank_descriptions_are_dropped() {
    let catalog = catalog(
        json!([
            {"plugin_id": "hunk", "action_id": "review", "title": "\tReview\nchanges\u{7}", "description": " \n\t ", "contexts": ["pane"]},
            {"plugin_id": "hunk", "action_id": "stage", "title": "Stage", "description": "", "contexts": ["pane"]},
            {"plugin_id": "hunk", "action_id": "undo", "title": "Undo", "description": "Reverts the last hunk.", "contexts": ["pane"]},
        ]),
        json!([{"plugin_id": "hunk", "name": "Hunk\r\nReview", "enabled": true}]),
    );
    let rows = catalog.rows(PANE);
    let review = row(&rows, "review");
    assert_eq!(review.title, "Review changes");
    assert_eq!(review.heading.as_deref(), Some("Hunk  Review"));
    assert_eq!(review.description, None);
    assert_eq!(row(&rows, "stage").description, None);
    assert_eq!(
        row(&rows, "undo").description.as_deref(),
        Some("Reverts the last hunk.")
    );
}
