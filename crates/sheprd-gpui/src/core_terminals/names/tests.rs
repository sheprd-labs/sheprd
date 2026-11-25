use crate::core_terminals::{checkouts::Checkouts, names::apply};
use sheprd_session::{CoreWorkspace, FolderStyle, Session, Worktree};
use sheprd_vcs::git::Identity;

fn session(custom_label: bool, linked: bool) -> Session {
    Session::core(
        [CoreWorkspace {
            branch: Some("main"),
            cwd: "/original",
            custom_label,
            id: "root",
            label: "original",
            worktree: Some(Worktree {
                key: "/original/.git".into(),
                label: "original".into(),
                linked,
            }),
        }],
        [],
        None,
        |_| None,
    )
}

#[test]
fn root_names_follow_checkouts_and_plain_folders_without_changing_identity() {
    let mut checkouts = Checkouts::default();
    for (cwd, root) in [
        ("/code/herdr-app/src", "/code/herdr-app"),
        ("/trees/feature/src", "/trees/feature"),
    ] {
        checkouts.seed(
            cwd,
            Some(Identity {
                branch: Some("feature".into()),
                key: "/code/herdr-app/.git".into(),
                label: "herdr-app".into(),
                linked: root.starts_with("/trees"),
                root: root.into(),
            }),
        );
    }
    for custom in [false, true] {
        for linked in [false, true] {
            let mut session = session(custom, linked);
            let original = session.workspaces[0].clone();
            for (cwd, expected) in [
                ("/code/herdr-app/src", "herdr-app"),
                ("/trees/feature/src", "feature"),
                ("/tmp/notes", "notes"),
                ("/Users/me", "~"),
            ] {
                session.workspaces[0].cwd = cwd.into();
                apply(
                    &mut session,
                    Some(&checkouts),
                    "/Users/me",
                    FolderStyle::Posix,
                );
                let mut expected_workspace = original.clone();
                expected_workspace.cwd = cwd.into();
                if !custom && !linked {
                    expected_workspace.label = expected.into();
                }
                assert_eq!(session.workspaces[0], expected_workspace);
            }
        }
    }
}

#[test]
fn missing_live_folders_use_the_saved_folder_and_stale_answers_are_not_used() {
    let mut checkouts = Checkouts::default();
    checkouts.seed(
        "/old/src",
        Some(Identity {
            branch: None,
            key: "/old/.git".into(),
            label: "old".into(),
            linked: false,
            root: "/old".into(),
        }),
    );
    let mut session = session(false, false);
    for cwd in [None, Some("/new/src")] {
        session.workspaces[0].cwd = cwd.unwrap_or("/original").into();
        apply(&mut session, Some(&checkouts), "", FolderStyle::Posix);
        assert_eq!(
            session.workspaces[0].label,
            if cwd.is_some() { "src" } else { "original" }
        );
    }
}

#[test]
fn home_naming_uses_the_owning_hosts_home() {
    let mut local = session(false, false);
    let mut remote = session(false, false);
    local.workspaces[0].cwd = "/home/alice".into();
    remote.workspaces[0].cwd = "/home/alice".into();
    apply(&mut local, None, "/Users/alice", FolderStyle::Posix);
    apply(&mut remote, None, "/home/alice", FolderStyle::Posix);
    assert_eq!(local.workspaces[0].label, "alice");
    assert_eq!(remote.workspaces[0].label, "~");
}
