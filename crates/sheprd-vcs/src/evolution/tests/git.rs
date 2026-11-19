//! Change history against real repositories, with Git isolated from the
//! user's configuration.

use super::*;
use sheprd_client::{ScriptHost, run_script};
use sheprd_usage::Host;
use std::{fs, path::Path, process::Command, sync::atomic::AtomicBool};

const ENV: &str = "GIT_CONFIG_GLOBAL=/dev/null\nGIT_CONFIG_NOSYSTEM=1\nexport GIT_CONFIG_GLOBAL GIT_CONFIG_NOSYSTEM\n";

fn isolated(_: &Checkout, body: &str, input: &[u8], limits: ScriptLimits) -> Result<String> {
    let mut output = Vec::new();
    run_script(
        ScriptHost::Local,
        &format!("{ENV}{body}"),
        input,
        &mut output,
        limits,
        &AtomicBool::new(false),
    )
    .map_err(crate::checkpoint::Error::Script)?;
    Ok(String::from_utf8(output).unwrap())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit_file(dir: &Path, file: &str, text: &str, subject: &str) {
    fs::write(dir.join(file), text).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", subject]);
}

/// A repository on `main` with one commit, and `feature` checked out on it.
fn repo() -> (tempfile::TempDir, std::path::PathBuf, Checkout) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("repo");
    fs::create_dir(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    commit_file(&dir, "base.txt", "base\n", "base");
    git(&dir, &["checkout", "-q", "-b", "feature"]);
    let key = git(&dir, &["rev-parse", "--absolute-git-dir"]);
    let checkout = Checkout::new(Host::Local, &key, Some("feature")).unwrap();
    (temp, dir, checkout)
}

fn history_of(checkout: &Checkout) -> History {
    history(checkout, 99, isolated).unwrap().unwrap()
}

fn evo_ref(dir: &Path) -> String {
    git(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/herdr-gpui/evolution/",
        ],
    )
}

#[test]
fn amends_and_rebases_are_recorded_and_their_commits_kept() {
    let (_temp, dir, checkout) = repo();
    let claude = Credit::Agents("Claude Code".into());
    commit_file(&dir, "one.txt", "one\n", "one");
    commit_file(&dir, "two.txt", "two\n", "two\n\nChange-Id: I2");
    observe(&checkout, &claude, 10, isolated).unwrap();
    let first = history_of(&checkout);
    assert_eq!(first.stack.len(), 2);
    assert_eq!(first.stack[1].change_id.as_deref(), Some("I2"));
    assert!(first.markers.is_empty());
    let original = first.stack[1].sha.clone();

    // Amended outside a turn, seen when the next one starts.
    fs::write(dir.join("two.txt"), "two, amended\n").unwrap();
    git(&dir, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    let pending = history_of(&checkout);
    assert_eq!(pending.pending, 1, "seen, not yet recorded");
    assert!(pending.is_pending(&pending.markers[0]));
    observe(&checkout, &Credit::Outside, 20, isolated).unwrap();
    let amended = history_of(&checkout);
    assert_eq!(amended.pending, 0);
    let amend = &amended.markers[0];
    assert_eq!(
        (amend.old.as_str(), amend.time, amend.by.as_ref()),
        (original.as_str(), 20, None)
    );
    assert!(matches!(
        amend.outcome,
        Outcome::Replaced {
            how: Match::ChangeId,
            ..
        }
    ));

    // Rebased onto a moved main in an agent's turn.
    git(&dir, &["checkout", "-q", "main"]);
    commit_file(&dir, "main.txt", "main\n", "main moved");
    git(&dir, &["checkout", "-q", "feature"]);
    git(&dir, &["rebase", "-q", "main"]);
    observe(&checkout, &claude, 30, isolated).unwrap();
    let rebased = history_of(&checkout);
    let top = &rebased.stack[1].sha;
    let chain: Vec<(&str, Option<&str>)> = versions(&rebased.markers, top)
        .iter()
        .map(|marker| (marker.old.as_str(), marker.by.as_deref()))
        .collect();
    assert_eq!(chain.len(), 2, "{chain:?}");
    assert_eq!(chain[0].1, Some("Claude Code"));
    assert_eq!(chain[1], (original.as_str(), None));

    // Git keeps every version the history names.
    git(&dir, &["reflog", "expire", "--expire=now", "--all"]);
    git(&dir, &["gc", "-q", "--prune=now"]);
    git(&dir, &["cat-file", "-e", &format!("{original}^{{commit}}")]);
    // Branch listings do not show it.
    assert!(!git(&dir, &["branch", "-a"]).contains("evolution"));
}

#[test]
fn a_write_from_a_stale_read_loses_and_reads_again() {
    let (_temp, dir, checkout) = repo();
    commit_file(&dir, "one.txt", "one\n", "one");
    observe(&checkout, &Credit::Outside, 1, isolated).unwrap();
    let head = git(&dir, &["rev-parse", &evo_ref(&dir)]);

    let stale = script::write(&checkout, Some(&sha('0')), &[]);
    let output = isolated(
        &checkout,
        &stale,
        Log::default().render().as_bytes(),
        crate::checkpoint::LIMITS,
    )
    .unwrap();
    assert!(!script::parse_write(&output).unwrap(), "{output}");
    let fresh = script::write(&checkout, None, &[]);
    let output = isolated(&checkout, &fresh, b"", crate::checkpoint::LIMITS).unwrap();
    assert!(!script::parse_write(&output).unwrap(), "it already exists");
    assert_eq!(git(&dir, &["rev-parse", &evo_ref(&dir)]), head, "untouched");
}

#[test]
fn merged_and_deleted_branches_forget_their_history() {
    let (_temp, dir, checkout) = repo();
    commit_file(&dir, "one.txt", "one\n", "one");
    observe(&checkout, &Credit::Outside, 1, isolated).unwrap();
    assert!(!evo_ref(&dir).is_empty());
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["merge", "-q", "--ff-only", "feature"]);
    git(&dir, &["checkout", "-q", "feature"]);
    observe(&checkout, &Credit::Outside, 2, isolated).unwrap();
    assert_eq!(evo_ref(&dir), "", "merged");

    // A deleted branch's history goes with the next write of another.
    git(&dir, &["checkout", "-q", "-b", "gone"]);
    commit_file(&dir, "gone.txt", "gone\n", "gone");
    let gone = Checkout::new(Host::Local, &checkout.repo_key, Some("gone")).unwrap();
    observe(&gone, &Credit::Outside, 3, isolated).unwrap();
    git(&dir, &["checkout", "-q", "feature"]);
    git(&dir, &["branch", "-q", "-D", "gone"]);
    commit_file(&dir, "two.txt", "two\n", "two");
    observe(&checkout, &Credit::Outside, 4, isolated).unwrap();
    assert_eq!(evo_ref(&dir).lines().count(), 1, "only feature's is left");
}

#[test]
fn the_default_branch_has_no_stack() {
    let (_temp, dir, _) = repo();
    git(&dir, &["checkout", "-q", "main"]);
    let key = git(&dir, &["rev-parse", "--absolute-git-dir"]);
    let main = Checkout::new(Host::Local, &key, Some("main")).unwrap();
    assert_eq!(history(&main, 0, isolated).unwrap(), None);
    observe(&main, &Credit::Outside, 0, isolated).unwrap();
    assert_eq!(evo_ref(&dir), "");
}

#[test]
fn untrusted_subjects_stay_data() {
    let (_temp, dir, checkout) = repo();
    commit_file(&dir, "one.txt", "one\n", "it's $(touch pwned) `x` \"q\"");
    observe(
        &checkout,
        &Credit::Agents("a'gent $(touch pwned)".into()),
        1,
        isolated,
    )
    .unwrap();
    assert_eq!(
        history_of(&checkout).stack[0].subject,
        "it's $(touch pwned) `x` \"q\""
    );
    assert!(!dir.join("pwned").exists());
}

#[test]
fn separators_in_a_subject_cannot_forge_output() {
    let (_temp, dir, checkout) = repo();
    let forged = format!("one\u{1e}skip\u{1f}{}", "a".repeat(40));
    commit_file(&dir, "one.txt", "one\n", &forged);
    observe(&checkout, &Credit::Outside, 1, isolated).unwrap();
    let history = history_of(&checkout);
    assert_eq!(history.stack.len(), 1);
    assert_eq!(
        history.stack[0].subject,
        format!("oneskip{}", "a".repeat(40))
    );
}

#[test]
fn a_range_diff_shows_only_what_the_rewrite_changed() {
    let (_temp, dir, checkout) = repo();
    commit_file(&dir, "one.txt", "one\n", "one");
    observe(&checkout, &Credit::Outside, 1, isolated).unwrap();
    let before = git(&dir, &["rev-parse", "HEAD"]);
    fs::write(dir.join("one.txt"), "one, amended\n").unwrap();
    git(&dir, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    let after = git(&dir, &["rev-parse", "HEAD"]);
    let history = history_of(&checkout);
    assert_eq!(history.current(&before), Some(after.as_str()));
    let text = diff(&checkout, &before, &after, isolated).unwrap();
    assert!(text.contains("one, amended"), "{text}");
    assert!(matches!(
        diff(&checkout, &before, "not a name", isolated),
        Err(crate::Error::Evolution(Error::Unknown))
    ));
    assert!(matches!(
        diff(&checkout, &before, &sha('0'), isolated),
        Err(crate::Error::Checkpoint(crate::checkpoint::Error::Script(
            _
        )))
    ));
}

#[test]
fn a_branch_stacked_on_an_amended_commit_is_stale_until_rebased() {
    let (_temp, dir, checkout) = repo();
    commit_file(&dir, "one.txt", "one\n", "one");
    observe(&checkout, &Credit::Outside, 1, isolated).unwrap();
    let old = git(&dir, &["rev-parse", "HEAD"]);
    git(&dir, &["branch", "stack/up"]);
    git(&dir, &["checkout", "-q", "stack/up"]);
    commit_file(&dir, "up.txt", "up\n", "up");
    git(&dir, &["checkout", "-q", "feature"]);
    fs::write(dir.join("one.txt"), "one, amended\n").unwrap();
    git(&dir, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    observe(&checkout, &Credit::Outside, 2, isolated).unwrap();
    let new = git(&dir, &["rev-parse", "HEAD"]);

    let found = stale(&checkout, 3, isolated).unwrap();
    assert_eq!(
        found,
        [Stale {
            branch: "stack/up".into(),
            old: old.clone(),
            new: Some(new.clone()),
        }]
    );
    git(&dir, &["rebase", "-q", "--onto", &new, &old, "stack/up"]);
    git(&dir, &["checkout", "-q", "feature"]);
    assert_eq!(stale(&checkout, 4, isolated).unwrap(), []);
}
