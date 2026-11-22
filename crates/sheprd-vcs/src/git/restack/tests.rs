#![allow(clippy::unwrap_used)]

use super::*;
use std::{path::Path, process::Command, time::Duration};

/// Git in `dir` with its own identity and no signing or hooks, whatever the
/// person running the tests has configured.
fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn configure(dir: &Path, hooks: &Path) {
    for (key, value) in [
        ("user.name", "Test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("rebase.autoStash", "false"),
        ("core.hooksPath", hooks.to_str().unwrap()),
    ] {
        git_in(dir, &["config", key, value]);
    }
}

fn commit(dir: &Path, file: &str, text: &str, message: &str) -> String {
    std::fs::write(dir.join(file), text).unwrap();
    git_in(dir, &["add", file]);
    git_in(dir, &["commit", "-m", message]);
    git_in(dir, &["rev-parse", "HEAD"])
}

/// A checkout on `layer-2`, built on `layer-1`, whose remote's `main` took
/// `layer-1` as a squash. Returns the folders kept alive and `layer-1`'s tip.
struct Stack {
    _remote: tempfile::TempDir,
    _hooks: tempfile::TempDir,
    checkout: tempfile::TempDir,
    below: String,
}

fn squashed_stack() -> Stack {
    let remote = tempfile::tempdir().unwrap();
    let hooks = tempfile::tempdir().unwrap();
    let checkout = tempfile::tempdir().unwrap();
    git_in(remote.path(), &["init", "--bare", "-b", "main"]);
    let dir = checkout.path();
    git_in(dir, &["init", "-b", "main"]);
    configure(dir, hooks.path());
    // Sheprd reads the GitHub repository from `origin`; Git fetches from the
    // local bare repository instead.
    git_in(
        dir,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/project.git",
        ],
    );
    git_in(
        dir,
        &[
            "config",
            &format!("url.{}.insteadOf", remote.path().display()),
            "https://github.com/example/project.git",
        ],
    );
    commit(dir, "base", "base\n", "base");
    git_in(dir, &["push", "origin", "main"]);
    git_in(dir, &["checkout", "-b", "layer-1"]);
    commit(dir, "one", "one\n", "layer 1, first");
    let below = commit(dir, "one", "one\nmore\n", "layer 1, second");
    git_in(dir, &["checkout", "-b", "layer-2"]);
    commit(dir, "two", "two\n", "layer 2");
    // GitHub squashes layer 1 into main.
    git_in(dir, &["checkout", "main"]);
    git_in(dir, &["merge", "--squash", "layer-1"]);
    git_in(dir, &["commit", "-m", "Layer 1 (#1361)"]);
    git_in(dir, &["push", "origin", "main"]);
    git_in(dir, &["reset", "--hard", "HEAD~1"]);
    git_in(dir, &["checkout", "layer-2"]);
    Stack {
        _remote: remote,
        _hooks: hooks,
        checkout,
        below,
    }
}

fn run_restack(stack: &Stack, below: &str, base: &str) -> crate::Result<Outcome> {
    restack(
        stack.checkout.path().to_str().unwrap(),
        "layer-2",
        below,
        base,
        Instant::now() + Duration::from_secs(30),
        &|| false,
    )
}

#[test]
fn restacking_keeps_only_the_branchs_own_commits_on_the_new_base() {
    let stack = squashed_stack();
    let dir = stack.checkout.path();
    let outcome = run_restack(&stack, &stack.below, "main").unwrap();
    assert!(
        outcome
            .message
            .starts_with("Restacked layer-2 onto origin/main")
    );
    assert_eq!(
        git_in(dir, &["rev-list", "--count", "origin/main..HEAD"]),
        "1"
    );
    assert_eq!(git_in(dir, &["log", "-1", "--format=%s"]), "layer 2");
    assert_eq!(
        git_in(dir, &["rev-parse", "HEAD~1"]),
        git_in(dir, &["rev-parse", "origin/main"])
    );
    // Run again, there is nothing left to do.
    let again = run_restack(&stack, &stack.below, "main").unwrap();
    assert!(again.message.starts_with("Nothing to restack"));
    assert_eq!(
        again.restacked,
        Some(Restacked::Unchanged {
            branch: "layer-2".into()
        })
    );
}

#[test]
fn a_dirty_checkout_is_refused_and_left_as_it_was() {
    let stack = squashed_stack();
    let dir = stack.checkout.path();
    let head = git_in(dir, &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("two"), "edited\n").unwrap();
    assert!(matches!(
        run_restack(&stack, &stack.below, "main"),
        Err(crate::Error::Git(super::super::Error::Restack(
            Error::Dirty
        )))
    ));
    assert_eq!(git_in(dir, &["rev-parse", "HEAD"]), head);
}

#[test]
fn a_conflict_leaves_the_rebase_stopped_for_the_user() {
    let stack = squashed_stack();
    let dir = stack.checkout.path();
    // Main changes the file layer 2 adds, so replaying it conflicts.
    git_in(dir, &["checkout", "origin/main", "-B", "conflict"]);
    commit(dir, "two", "main's own\n", "main touches two");
    git_in(dir, &["push", "origin", "conflict:main"]);
    git_in(dir, &["checkout", "layer-2"]);
    let result = run_restack(&stack, &stack.below, "main");
    assert!(matches!(
        result,
        Err(crate::Error::Git(super::super::Error::Restack(
            Error::Stopped { .. }
        )))
    ));
    let rebasing = dir.join(".git/rebase-merge").exists() || dir.join(".git/rebase-apply").exists();
    assert!(rebasing, "the rebase stays stopped, not aborted");
}

#[test]
fn only_commit_ids_and_plain_branch_names_reach_git() {
    let stack = squashed_stack();
    for (below, base) in [
        ("--upload-pack=evil", "main"),
        ("HEAD~1", "main"),
        (stack.below.as_str(), "-x"),
        (stack.below.as_str(), "main..other"),
        (stack.below.as_str(), "ma in"),
    ] {
        assert!(
            matches!(
                run_restack(&stack, below, base),
                Err(crate::Error::Git(super::super::Error::Restack(
                    Error::Target
                )))
            ),
            "{below} {base}"
        );
    }
    assert!(branch_name("feat/t3-client"));
    assert!(!branch_name("refs/heads/x.lock"));
}

#[test]
fn a_checkout_on_another_branch_is_refused_before_anything_moves() {
    let stack = squashed_stack();
    let dir = stack.checkout.path();
    git_in(dir, &["checkout", "layer-1"]);
    let head = git_in(dir, &["rev-parse", "HEAD"]);
    assert!(matches!(
        run_restack(&stack, &stack.below, "main"),
        Err(crate::Error::Git(super::super::Error::Restack(
            Error::Moved
        )))
    ));
    assert_eq!(git_in(dir, &["rev-parse", "HEAD"]), head);
}

#[test]
fn other_local_branches_stay_where_they_were_even_with_update_refs_on() {
    let stack = squashed_stack();
    let dir = stack.checkout.path();
    git_in(dir, &["config", "rebase.updateRefs", "true"]);
    // A saved branch on a commit the restack replays.
    git_in(dir, &["branch", "saved", "HEAD"]);
    let saved = git_in(dir, &["rev-parse", "saved"]);
    let outcome = run_restack(&stack, &stack.below, "main").unwrap();
    assert!(matches!(outcome.restacked, Some(Restacked::Onto { .. })));
    assert_eq!(git_in(dir, &["rev-parse", "saved"]), saved);
}

#[test]
fn a_branch_whose_own_commits_include_a_merge_is_left_for_a_person() {
    let stack = squashed_stack();
    let dir = stack.checkout.path();
    git_in(dir, &["checkout", "-b", "side", "layer-2"]);
    commit(dir, "side", "side\n", "side work");
    git_in(dir, &["checkout", "layer-2"]);
    commit(dir, "three", "three\n", "layer 2, more");
    git_in(dir, &["merge", "--no-ff", "-m", "merge side", "side"]);
    let head = git_in(dir, &["rev-parse", "HEAD"]);
    assert!(matches!(
        run_restack(&stack, &stack.below, "main"),
        Err(crate::Error::Git(super::super::Error::Restack(
            Error::Merges
        )))
    ));
    assert_eq!(git_in(dir, &["rev-parse", "HEAD"]), head);
}

#[test]
fn edits_are_refused_rather_than_stashed_and_the_fetched_base_is_forgotten() {
    let stack = squashed_stack();
    let dir = stack.checkout.path();
    git_in(dir, &["config", "rebase.autoStash", "true"]);
    std::fs::write(dir.join("two"), "edited\n").unwrap();
    assert!(matches!(
        run_restack(&stack, &stack.below, "main"),
        Err(crate::Error::Git(super::super::Error::Restack(
            Error::Dirty
        )))
    ));
    assert_eq!(git_in(dir, &["stash", "list"]), "");
    git_in(dir, &["checkout", "--", "two"]);
    run_restack(&stack, &stack.below, "main").unwrap();
    let refs = git_in(dir, &["for-each-ref", "refs/sheprd"]);
    assert_eq!(refs, "", "the action's own ref is deleted");
}

#[test]
fn every_restack_fetches_into_a_ref_of_its_own() {
    let first = onto_ref();
    let second = onto_ref();
    assert_ne!(first, second);
    assert!(first.starts_with("refs/sheprd/restack/"));
    assert!(
        first
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/-".contains(&byte))
    );
}
