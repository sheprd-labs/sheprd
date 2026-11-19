#![allow(clippy::unwrap_used)]

use super::*;
use sheprd_vcs::evolution::Commit;

fn sha(digit: char) -> String {
    digit.to_string().repeat(40)
}

fn commit(digit: char, subject: &str) -> Commit {
    Commit {
        sha: sha(digit),
        patch_id: None,
        change_id: None,
        subject: subject.into(),
    }
}

fn marker(old: char, outcome: Outcome, by: Option<&str>, time: i64) -> Marker {
    Marker {
        time,
        old: sha(old),
        outcome,
        by: by.map(str::to_owned),
    }
}

fn replaced(new: char, how: Match) -> Outcome {
    Outcome::Replaced { new: sha(new), how }
}

/// `one`, then `two` amended in Codex's turn and rebased since the last
/// turn; `gone` dropped outside a turn.
fn history() -> History {
    History {
        stack: vec![commit('a', "one"), commit('c', "two")],
        markers: vec![
            marker('b', replaced('d', Match::Subject), Some("Codex"), 1_000),
            marker('e', Outcome::Dropped, None, 2_000),
            marker('d', replaced('c', Match::Patch), None, 3_600),
        ],
        pending: 1,
    }
}

#[test]
fn the_log_lists_commits_newest_first_with_their_versions() {
    let log = change_log(&history(), None);
    let shas: Vec<&str> = log.commits.iter().map(|commit| &commit.sha[..1]).collect();
    assert_eq!(shas, ["c", "a"]);
    assert_eq!(
        log.commits[0].versions,
        [
            VersionInfo {
                sha: sha('d'),
                replaced: 3_600,
                how: How::Patch,
                by: None,
                pending: true,
            },
            VersionInfo {
                sha: sha('b'),
                replaced: 1_000,
                how: How::Subject,
                by: Some("Codex".into()),
                pending: false,
            },
        ]
    );
    assert_eq!(log.dropped.len(), 1);
    assert_eq!(
        describe(&log, "feature", 3_600 + 7_200),
        "ccccccc  two  (3 versions)
    was ddddddd  rebased or reworded, not recorded yet
    was bbbbbbb  amended in Codex's turn, 2 h ago
aaaaaaa  one
Dropped:
    eeeeeee  dropped outside an agent's turn, 2 h ago"
    );
}

#[test]
fn a_prefix_shows_one_commit_and_no_drops() {
    let log = change_log(&history(), Some("aaaa"));
    assert_eq!(log.commits.len(), 1);
    assert!(log.commits[0].versions.is_empty());
    assert!(log.dropped.is_empty());
    assert!(change_log(&history(), Some("ffff")).commits.is_empty());
    assert_eq!(
        describe(&ChangeLog::default(), "feature", 0),
        "feature has no commits of its own"
    );
}

#[test]
fn json_names_how_in_kebab_case() {
    let json = serde_json::to_value(change_log(&history(), None)).unwrap();
    assert_eq!(json["commits"][0]["versions"][0]["how"], "patch");
    assert_eq!(json["dropped"][0]["how"], "dropped");
    assert_eq!(serde_json::to_value(How::ChangeId).unwrap(), "change-id");
}

#[test]
fn stale_branches_read_with_their_rebase() {
    let stale = [
        StaleInfo {
            branch: "stack/up".into(),
            old: sha('a'),
            new: Some(sha('b')),
            command: Some("git rebase --onto bbbb aaaa stack/up".into()),
        },
        StaleInfo {
            branch: "other".into(),
            old: sha('c'),
            new: None,
            command: None,
        },
    ];
    assert_eq!(
        describe_stale(&stale, "feature"),
        "stack/up is built on aaaaaaa, now bbbbbbb
    git rebase --onto bbbb aaaa stack/up
other is built on ccccccc, which was dropped from feature"
    );
    assert_eq!(
        describe_stale(&[], "feature"),
        "No other branch is built on an earlier version of feature's commits"
    );
}
