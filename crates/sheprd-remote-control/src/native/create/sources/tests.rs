use super::*;

const LISTING: &str = "worktree /code/app
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /home/me/.herdr/worktrees/app/feature
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature/x

worktree /home/me/.herdr/worktrees/app/detached
HEAD 3333333333333333333333333333333333333333
detached

worktree /tmp/gone
HEAD 4444444444444444444444444444444444444444
branch refs/heads/gone
prunable gitdir file points to non-existent location
";

/// `LISTING` as `--porcelain -z` writes it.
fn nul(listing: &str) -> String {
    listing.replace('\n', "\0")
}

#[test]
fn worktrees_are_listed_with_their_branches_skipping_prunable_ones() {
    let worktrees = parse_worktrees(&nul(LISTING));
    assert_eq!(
        worktrees.linked,
        vec![
            Listed {
                path: "/home/me/.herdr/worktrees/app/feature".into(),
                branch: Some("feature/x".into())
            },
            Listed {
                path: "/home/me/.herdr/worktrees/app/detached".into(),
                branch: None
            },
        ]
    );
    assert!(worktrees.branches.contains("main") && worktrees.branches.contains("gone"));
}

#[test]
fn a_bare_repository_keeps_its_first_linked_checkout() {
    let listing =
        nul("worktree /code/app.git\nbare\n\nworktree /w/one\nHEAD 1\nbranch refs/heads/one\n\n");
    let worktrees = parse_worktrees(&listing);
    assert_eq!(
        worktrees.linked,
        vec![Listed {
            path: "/w/one".into(),
            branch: Some("one".into())
        }]
    );
}

#[test]
fn a_path_git_would_quote_comes_as_it_is() {
    let listing = nul("worktree /code/app\n\nworktree /w/a \"b\"\\c\nHEAD 1\ndetached\n\n");
    assert_eq!(parse_worktrees(&listing).linked[0].path, "/w/a \"b\"\\c");
}

#[test]
fn free_branches_leave_out_checked_out_and_unsendable_ones() {
    let taken = parse_worktrees(&nul(LISTING)).branches;
    let refs = "refs/heads/feature/x\nrefs/heads/fix-1\nrefs/heads/main\nrefs/heads/odd name\nrefs/heads/release/2\n";
    assert_eq!(free_branches(refs, &taken), vec!["fix-1", "release/2"]);
}

#[test]
fn a_pull_request_head_may_be_any_branch_git_allows() {
    for good in ["fix+logging", "修正", "a/b-c_d.e", "v1@2"] {
        assert!(git_branch(good), "{good}");
        assert!(pull_refs("origin", 1, good, false).is_some(), "{good}");
    }
    for bad in [
        "",
        "@",
        "-x",
        "/x",
        "x/",
        "x.",
        "a..b",
        "a@{1}",
        "a//b",
        "a b",
        "a~1",
        "a^",
        "a:b",
        "a?",
        "a*",
        "a[",
        "a\\b",
        ".hidden",
        "a/.b",
        "x.lock",
        "a/b.lock/c",
        "a\tb",
    ] {
        assert!(!git_branch(bad), "{bad:?}");
    }
}

#[test]
fn a_pull_request_is_fetched_where_the_desktop_fetches_it() {
    let (same, refspec) = pull_refs("origin", 12, "feature/y", false).unwrap();
    assert_eq!(
        same,
        Fetched {
            branch: "feature/y".into(),
            base: "refs/remotes/origin/feature/y".into()
        }
    );
    assert_eq!(
        refspec,
        "+refs/heads/feature/y:refs/remotes/origin/feature/y"
    );
    let (fork, refspec) = pull_refs("upstream", 7, "main", true).unwrap();
    assert_eq!(
        fork,
        Fetched {
            branch: "pr/7".into(),
            base: "refs/herdr/pull/7/head".into()
        }
    );
    assert_eq!(refspec, "+refs/pull/7/head:refs/herdr/pull/7/head");
}

#[test]
fn a_head_branch_that_could_be_an_option_is_refused() {
    assert!(pull_refs("origin", 1, "--upload-pack=x", false).is_none());
    assert!(pull_refs("origin", 1, "a..b", false).is_none());
}

#[test]
fn a_request_names_its_source_by_kind() {
    let pull: Source =
        serde_json::from_str(r#"{"kind":"pull","number":3,"head":"x","head_owner":"me"}"#).unwrap();
    assert_eq!(
        pull,
        Source::Pull {
            number: 3,
            head: "x".into(),
            head_owner: Some("me".into())
        }
    );
    let checkout: Source = serde_json::from_str(r#"{"kind":"checkout","path":"/a"}"#).unwrap();
    assert_eq!(checkout, Source::Checkout { path: "/a".into() });
}

#[test]
fn only_a_listed_linked_checkout_can_be_opened() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("app");
    std::fs::create_dir(&repository).unwrap();
    let run = |args: &[&str]| {
        let status = sheprd_client::new_command("git")
            .current_dir(&repository)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    run(&["init", "--quiet"]);
    run(&["commit", "--quiet", "--allow-empty", "-m", "start"]);
    let linked = dir.path().join("linked");
    run(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        "side",
        linked.to_str().unwrap(),
    ]);
    run(&["branch", "free"]);
    let found = checkout_of(&repository, linked.to_str().unwrap()).unwrap();
    assert_eq!(canonical(Path::new(&found)), canonical(&linked));
    assert!(matches!(
        checkout_of(&repository, repository.to_str().unwrap()),
        Err(Error::NotACheckout)
    ));
    assert!(matches!(
        checkout_of(&repository, "/etc"),
        Err(Error::NotACheckout)
    ));
    let sources = read_sources(&repository, &[]).unwrap();
    assert_eq!(sources.branches, vec!["free"]);
    assert_eq!(sources.checkouts.len(), 1);
    let open = [linked.to_str().unwrap()];
    assert!(
        read_sources(&repository, &open)
            .unwrap()
            .checkouts
            .is_empty()
    );
}

fn remote(owner: &str) -> Option<Remote> {
    Some(Remote {
        host: "github.com".into(),
        owner: owner.into(),
        repo: "app".into(),
    })
}

#[test]
fn a_pull_request_head_is_fetched_from_the_remote_that_has_it() {
    let fork_checkout = checkouts::Checkout {
        origin: remote("me"),
        upstream: remote("acme"),
        ..Default::default()
    };
    // The listed repository's own branch.
    assert_eq!(
        pull_remote(&fork_checkout, Some("Acme")),
        Some(("upstream", false))
    );
    // This checkout's own fork: an ordinary branch of origin.
    assert_eq!(
        pull_remote(&fork_checkout, Some("me")),
        Some(("origin", false))
    );
    // Anyone else's fork, or one GitHub no longer names.
    assert_eq!(
        pull_remote(&fork_checkout, Some("other")),
        Some(("upstream", true))
    );
    assert_eq!(pull_remote(&fork_checkout, None), Some(("upstream", true)));
    let plain = checkouts::Checkout {
        origin: remote("acme"),
        ..Default::default()
    };
    assert_eq!(pull_remote(&plain, Some("acme")), Some(("origin", false)));
    assert_eq!(pull_remote(&plain, Some("me")), Some(("origin", true)));
    assert_eq!(
        pull_remote(&checkouts::Checkout::default(), Some("acme")),
        None
    );
}
