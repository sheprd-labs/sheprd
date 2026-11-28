use super::*;
use crate::{paths::Paths, report::By};
use std::{fs, process::ExitCode};

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn output(command: &Command, paths: &Paths) -> (ExitCode, String) {
    let mut out = Vec::new();
    let code = run::run_in(command, paths, &mut out).unwrap();
    (code, String::from_utf8(out).unwrap())
}

const NIGHTLY: &str = r#"
herdr.caps { agents_per_run = 1, tokens_per_day = 2_000_000, wall_time = "3h" }
herdr.at("03:00", { tz = "Europe/Paris" }, function(fire)
  pull("main")
  sh("just ci")
  agent { harness = "claude", worktree = "nightly-" .. fire.date, prompt = "Review main." }
end)
"#;

#[test]
fn parses_every_command() {
    let cases: Vec<(&[&str], Command)> = vec![
        (
            &["check", "a.luau", "--json"],
            Command::Check {
                file: "a.luau".into(),
                json: true,
            },
        ),
        (&["status"], Command::Status { json: false }),
        (&["list", "--json"], Command::Status { json: true }),
        (
            &["approve", "a.luau", "--hash", "abc"],
            Command::Approve {
                file: "a.luau".into(),
                hash: Some("abc".to_owned()),
                json: false,
            },
        ),
        (
            &["run-now", "a.luau", "--trigger", "deploy", "--json"],
            Command::RunNow {
                file: "a.luau".into(),
                trigger: Some("deploy".to_owned()),
                json: true,
            },
        ),
        (
            &["pause", "--all"],
            Command::Pause {
                target: Target::All,
                json: false,
            },
        ),
        (
            &["resume", "a.luau"],
            Command::Resume {
                target: Target::Script("a.luau".into()),
                json: false,
            },
        ),
        (
            &["runs"],
            Command::Runs {
                script: None,
                json: false,
            },
        ),
        (
            &["runs", "show", "7", "--json"],
            Command::RunShow { id: 7, json: true },
        ),
        (
            &["log", "7", "--step", "2"],
            Command::Log {
                target: LogTarget::Run {
                    id: 7,
                    step: Some(2),
                },
                json: false,
                follow: false,
            },
        ),
        (
            &["log", "--daemon", "--lines", "99999", "--follow"],
            Command::Log {
                target: LogTarget::Daemon {
                    lines: MAX_LOG_LINES,
                },
                json: false,
                follow: true,
            },
        ),
        (
            &["stop", "3"],
            Command::Stop {
                run: 3,
                json: false,
            },
        ),
        (
            &["repo", "add", "/r"],
            Command::Repo {
                action: RepoAction::Add("/r".into()),
                json: false,
            },
        ),
        (
            &["repo"],
            Command::Repo {
                action: RepoAction::List,
                json: false,
            },
        ),
        (
            &["usage", "--since", "7d", "--by", "run"],
            Command::Usage {
                script: None,
                since: Since::Week,
                by: By::Run,
                json: false,
            },
        ),
        (&["serve"], Command::Serve),
        (&["start", "--json"], Command::Start { json: true }),
        (&["shutdown"], Command::Shutdown { json: false }),
        (
            &["install-service"],
            Command::InstallService { json: false },
        ),
        (&["watch"], Command::Watch),
        (&["--version"], Command::Version),
        (&["help"], Command::Help),
    ];
    for (input, expected) in cases {
        assert_eq!(parse(args(input)), Ok(expected), "{input:?}");
    }
}

#[test]
fn refuses_bad_usage() {
    assert_eq!(parse(args(&[])), Err(Error::NoCommand));
    assert_eq!(parse(args(&["check"])), Err(Error::Missing("file")));
    assert_eq!(
        parse(args(&["check", "a", "b"])),
        Err(Error::Unexpected("b".to_owned()))
    );
    assert_eq!(
        parse(args(&["status", "--hash", "x"])),
        Err(Error::Unexpected("--hash".to_owned()))
    );
    assert_eq!(
        parse(args(&["stop", "seven"])).unwrap_err().to_string(),
        "`run` does not take `seven`"
    );
    assert_eq!(parse(args(&["runs", "show"])), Err(Error::Missing("run")));
    assert!(parse(args(&["log", "--daemon", "--json", "--follow"])).is_err());
    assert!(parse(args(&["usage", "--since", "1y"])).is_err());
    assert_eq!(
        parse(args(&["chek"])),
        Err(Error::UnknownCommand("chek".to_owned()))
    );
    assert_eq!(
        parse(args(&["serve", "--json"])),
        Err(Error::Unexpected("--json".to_owned()))
    );
}

#[cfg(unix)]
#[test]
fn takes_paths_that_are_not_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(vec![b'a', 0xff, b'.', b'l']);
    assert_eq!(
        parse([OsString::from("check"), path.clone()]),
        Ok(Command::Check {
            file: PathBuf::from(path),
            json: false,
        })
    );
}

#[test]
fn check_prints_the_plan_and_its_hash() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path());
    let file = dir.path().join("nightly-validate.luau");
    fs::write(&file, NIGHTLY).unwrap();
    let (code, text) = output(&Command::Check { file, json: false }, &paths);
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(text.starts_with("nightly-validate\n"), "{text}");
    assert!(
        text.contains("    2. sh `just ci` (arbitrary command)\n"),
        "{text}"
    );
    assert!(
        text.contains("OK: 1 trigger. Every trigger can also be started by hand."),
        "{text}"
    );
    assert!(text.contains("sha256 "), "{text}");
}

#[test]
fn errors_are_json_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path());
    let file = dir.path().join("broken.luau");
    fs::write(&file, "herdr.manual(function() end)").unwrap();
    let (code, text) = output(&Command::Check { file, json: true }, &paths);
    assert_eq!(code, ExitCode::FAILURE);
    let value = serde_json::from_str::<serde_json::Value>(&text).unwrap();
    assert_eq!(value["ok"], false);
    assert!(
        value["message"].as_str().unwrap().contains("no caps"),
        "{value}"
    );
    let (code, text) = output(&Command::RunShow { id: 42, json: true }, &paths);
    assert_eq!(code, ExitCode::FAILURE);
    assert!(text.contains("\"message\":\"no run 42\""), "{text}");
}

#[test]
fn status_works_with_no_daemon_and_no_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path());
    let (code, text) = output(&Command::Status { json: true }, &paths);
    assert_eq!(code, ExitCode::SUCCESS);
    let status = serde_json::from_str::<sheprd_automation::report::Status>(&text).unwrap();
    assert!(!status.daemon.running);
    assert!(status.scripts.is_empty());
    assert_eq!(status.protocol, sheprd_automation::report::PROTOCOL);
}

#[test]
fn steps_lists_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let (_, text) = output(&Command::Steps { json: true }, &Paths::under(dir.path()));
    let value = serde_json::from_str::<serde_json::Value>(&text).unwrap();
    let names = value["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let expected = sheprd_automation::Kind::ALL
        .iter()
        .map(|kind| kind.name().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
}

#[cfg(unix)]
#[test]
fn a_script_is_approved_by_hash_then_can_be_run_by_hand() {
    use crate::approve::{self, Mode};
    let fixture = crate::testing::Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "deploy",
        "herdr.manual(function() notify(\"go\") end)\n",
    );
    let paths = &fixture.paths;
    let repo_add = Command::Repo {
        action: RepoAction::Add(repo.clone()),
        json: false,
    };
    assert_eq!(output(&repo_add, paths).0, ExitCode::SUCCESS);
    let store = fixture.store();
    let source = sheprd_automation::Source::read(&script).unwrap();
    let approve = |hash: &str, panes: approve::Panes<'_>| {
        approve::approve(
            paths,
            &store,
            &fixture.herdr,
            &script,
            Mode::Hash(hash),
            panes,
            |_| Ok(None),
        )
    };
    assert!(matches!(
        approve(&"0".repeat(64), approve::Panes::default()),
        Err(crate::Error::Approve(approve::Error::Changed { .. }))
    ));
    // Inside a Herdr pane or a Herdr 2 terminal alike.
    for panes in [
        approve::Panes::of(Some("w1p1"), None),
        approve::Panes::of(Some(""), Some("t3")),
    ] {
        assert!(matches!(
            approve(&source.hash, panes),
            Err(crate::Error::Approve(approve::Error::HashInPane))
        ));
    }
    assert_eq!(
        approve(&source.hash, approve::Panes::default())
            .unwrap()
            .hash,
        source.hash
    );
    let (code, text) = output(&Command::Status { json: true }, paths);
    assert_eq!(code, ExitCode::SUCCESS);
    let status = serde_json::from_str::<sheprd_automation::report::Status>(&text).unwrap();
    assert_eq!(
        status.scripts.first().map(|script| script.state),
        Some(sheprd_automation::report::ScriptState::Active)
    );
    let run_now = Command::RunNow {
        file: script.clone(),
        trigger: None,
        json: true,
    };
    let (code, text) = output(&run_now, paths);
    assert_eq!(code, ExitCode::SUCCESS, "{text}");
    assert!(text.contains("the daemon is not running"), "{text}");
    // Changing the script takes the approval away.
    fs::write(
        &script,
        format!("{}-- changed\n", fs::read_to_string(&script).unwrap()),
    )
    .unwrap();
    store
        .put_script(&crate::scan::plan_row(
            &sheprd_automation::Source::read(&script).unwrap(),
            script
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            None,
            0,
        ))
        .unwrap();
    let (code, text) = output(&run_now, paths);
    assert_eq!(code, ExitCode::FAILURE);
    assert!(text.contains("not approved"), "{text}");
}

#[test]
fn commands_that_leave_scripts_to_run_start_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let with_scripts = dir.path().join("with");
    fs::create_dir_all(with_scripts.join(".herdr/automations")).unwrap();
    fs::write(with_scripts.join(".herdr/automations/nightly.luau"), "").unwrap();
    let without = dir.path().join("without");
    fs::create_dir_all(&without).unwrap();
    let repo_add = |repo: &std::path::Path| Command::Repo {
        action: RepoAction::Add(repo.to_path_buf()),
        json: false,
    };
    assert!(run::leaves_scripts_to_run(&repo_add(&with_scripts)));
    assert!(!run::leaves_scripts_to_run(&repo_add(&without)));
    for command in [
        Command::Approve {
            file: "a.luau".into(),
            hash: None,
            json: true,
        },
        Command::RunNow {
            file: "a.luau".into(),
            trigger: None,
            json: false,
        },
    ] {
        assert!(run::leaves_scripts_to_run(&command), "{command:?}");
    }
    for command in [
        Command::Status { json: false },
        Command::Steps { json: false },
    ] {
        assert!(!run::leaves_scripts_to_run(&command), "{command:?}");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_scripts_are_found_approved_and_controlled() {
    use crate::{
        approve::{self, Mode},
        scan,
    };
    use std::os::unix::fs::symlink;

    let fixture = crate::testing::Fixture::new();
    let repo = fixture.repo();
    let paths = &fixture.paths;
    let repo_add = Command::Repo {
        action: RepoAction::Add(repo.clone()),
        json: false,
    };
    assert_eq!(output(&repo_add, paths).0, ExitCode::SUCCESS);

    let external = fixture.root().join("external");
    fs::create_dir_all(&external).unwrap();
    let body = "herdr.caps { agents_per_run = 1, tokens_per_day = 100_000, wall_time = \"10m\" }\nherdr.manual(function() notify(\"go\") end)\n";
    let external_personal = external.join("shared-personal.luau");
    let external_repo = external.join("shared-repo.luau");
    fs::write(&external_personal, body).unwrap();
    fs::write(&external_repo, body).unwrap();

    fs::create_dir_all(paths.personal()).unwrap();
    let personal_link = paths.personal().join("personal-link.luau");
    let repo_link = repo.join(".herdr/automations/repo-link.luau");
    symlink(&external_personal, &personal_link).unwrap();
    symlink(&external_repo, &repo_link).unwrap();

    let store = fixture.store();
    let found = scan::find(paths, &store).unwrap();
    assert_eq!(
        found,
        vec![
            scan::Found {
                path: personal_link.clone(),
                repo: None,
            },
            scan::Found {
                path: repo_link.clone(),
                repo: Some(repo.clone()),
            },
        ]
    );
    for item in &found {
        let source = sheprd_automation::Source::read(&item.path).unwrap();
        store
            .put_script(&scan::plan_row(
                &source,
                item.path.to_string_lossy().into_owned(),
                item.repo.as_deref(),
                0,
            ))
            .unwrap();
    }

    // Approve the personal symlink directly and the repo symlink via a relative path.
    let personal_source = sheprd_automation::Source::read(&personal_link).unwrap();
    approve::approve(
        paths,
        &store,
        &fixture.herdr,
        &personal_link,
        Mode::Hash(&personal_source.hash),
        approve::Panes::default(),
        |_| Ok(None),
    )
    .unwrap();

    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert_eq!(
        scan::script_path(std::path::Path::new("Cargo.toml")).unwrap(),
        cwd.join("Cargo.toml")
    );
    let mut cwd_parts = cwd.components().peekable();
    let mut link_parts = repo_link.components().peekable();
    while cwd_parts.peek() == link_parts.peek() && cwd_parts.peek().is_some() {
        cwd_parts.next();
        link_parts.next();
    }
    let mut relative_repo_link = PathBuf::new();
    for _ in cwd_parts {
        relative_repo_link.push("..");
    }
    relative_repo_link.extend(link_parts);
    assert!(!relative_repo_link.is_absolute());

    let repo_source = sheprd_automation::Source::read(&repo_link).unwrap();
    approve::approve(
        paths,
        &store,
        &fixture.herdr,
        &relative_repo_link,
        Mode::Hash(&repo_source.hash),
        approve::Panes::default(),
        |_| Ok(None),
    )
    .unwrap();

    assert_eq!(store.scripts().unwrap().len(), 2);
    let (code, text) = output(&Command::Status { json: true }, paths);
    assert_eq!(code, ExitCode::SUCCESS);
    let status = serde_json::from_str::<sheprd_automation::report::Status>(&text).unwrap();
    assert_eq!(status.scripts.len(), 2);
    assert!(
        status
            .scripts
            .iter()
            .all(|script| script.state == sheprd_automation::report::ScriptState::Active)
    );

    for script in [&personal_link, &repo_link] {
        let (code, text) = output(
            &Command::RunNow {
                file: script.clone(),
                trigger: None,
                json: true,
            },
            paths,
        );
        assert_eq!(code, ExitCode::SUCCESS, "{text}");
        let (code, text) = output(
            &Command::Pause {
                target: Target::Script(script.clone()),
                json: true,
            },
            paths,
        );
        assert_eq!(code, ExitCode::SUCCESS, "{text}");
        let (code, text) = output(
            &Command::Resume {
                target: Target::Script(script.clone()),
                json: true,
            },
            paths,
        );
        assert_eq!(code, ExitCode::SUCCESS, "{text}");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_automations_folder_keeps_a_single_store_row() {
    use crate::{
        approve::{self, Mode},
        scan,
    };
    use std::os::unix::fs::symlink;

    let fixture = crate::testing::Fixture::new();
    let paths = &fixture.paths;
    let real_personal = fixture.root().join("dotfiles-automations");
    fs::create_dir_all(&real_personal).unwrap();
    fs::create_dir_all(&paths.config).unwrap();
    symlink(&real_personal, paths.personal()).unwrap();

    let script = real_personal.join("nightly.luau");
    fs::write(
        &script,
        "herdr.caps { agents_per_run = 1, tokens_per_day = 100_000, wall_time = \"10m\" }\nherdr.manual(function() notify(\"go\") end)\n",
    )
    .unwrap();

    let store = fixture.store();
    let found = scan::find(paths, &store).unwrap();
    assert_eq!(
        found,
        vec![scan::Found {
            path: script.clone(),
            repo: None,
        }]
    );
    let source = sheprd_automation::Source::read(&script).unwrap();
    store
        .put_script(&scan::plan_row(
            &source,
            found[0].path.to_string_lossy().into_owned(),
            None,
            0,
        ))
        .unwrap();

    let via_symlink_dir = paths.personal().join("nightly.luau");
    approve::approve(
        paths,
        &store,
        &fixture.herdr,
        &via_symlink_dir,
        Mode::Hash(&source.hash),
        approve::Panes::default(),
        |_| Ok(None),
    )
    .unwrap();

    let rows = store.scripts().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, script.to_string_lossy());
    let (code, text) = output(
        &Command::RunNow {
            file: via_symlink_dir,
            trigger: None,
            json: true,
        },
        paths,
    );
    assert_eq!(code, ExitCode::SUCCESS, "{text}");
}

#[cfg(unix)]
#[test]
fn shared_automations_folder_keeps_its_first_repository() {
    use crate::{paths::repository_scripts, scan};
    use std::os::unix::fs::symlink;

    let fixture = crate::testing::Fixture::new();
    let paths = &fixture.paths;
    let repo_a = fixture.root().join("repo-a");
    let repo_b = fixture.root().join("repo-b");
    let folder_a = repository_scripts(&repo_a);
    fs::create_dir_all(&folder_a).unwrap();
    fs::create_dir_all(repository_scripts(&repo_b).parent().unwrap()).unwrap();
    symlink(&folder_a, repository_scripts(&repo_b)).unwrap();
    let script = folder_a.join("nightly.luau");
    fs::write(
        &script,
        "herdr.caps { agents_per_run = 1, tokens_per_day = 100_000, wall_time = \"10m\" }\nherdr.manual(function() notify(\"go\") end)\n",
    )
    .unwrap();

    let store = fixture.store();
    let repo_a = fs::canonicalize(&repo_a).unwrap();
    let repo_b = fs::canonicalize(&repo_b).unwrap();
    store.add_repo(&repo_a.to_string_lossy(), 0).unwrap();
    store.add_repo(&repo_b.to_string_lossy(), 0).unwrap();

    let found = scan::find(paths, &store).unwrap();
    assert_eq!(
        found,
        vec![scan::Found {
            path: fs::canonicalize(&script).unwrap(),
            repo: Some(repo_a),
        }]
    );
}

#[cfg(unix)]
#[test]
fn external_symlinks_and_error_paths_for_script_location() {
    use crate::{
        approve::{self, Mode},
        scan,
    };
    use std::os::unix::fs::symlink;

    let fixture = crate::testing::Fixture::new();
    let paths = &fixture.paths;
    let store = fixture.store();
    fs::create_dir_all(paths.personal()).unwrap();

    let body = "herdr.caps { agents_per_run = 1, tokens_per_day = 100_000, wall_time = \"10m\" }\nherdr.manual(function() notify(\"go\") end)\n";
    let personal_script = paths.personal().join("regular.luau");
    fs::write(&personal_script, body).unwrap();

    // External symlink pointing into the automations folder resolves via canonicalize fallback.
    let external_link = fixture.root().join("shortcut.luau");
    symlink(&personal_script, &external_link).unwrap();
    let source = sheprd_automation::Source::read(&personal_script).unwrap();
    approve::approve(
        paths,
        &store,
        &fixture.herdr,
        &external_link,
        Mode::Hash(&source.hash),
        approve::Panes::default(),
        |_| Ok(None),
    )
    .unwrap();
    assert_eq!(store.scripts().unwrap().len(), 1);
    let (code, text) = output(
        &Command::Pause {
            target: Target::Script(external_link),
            json: true,
        },
        paths,
    );
    assert_eq!(code, ExitCode::SUCCESS, "{text}");

    // Regular file outside any automations folder is rejected.
    let outside = fixture.root().join("outside.luau");
    fs::write(&outside, body).unwrap();
    assert!(matches!(
        approve::approve(
            paths,
            &store,
            &fixture.herdr,
            &outside,
            Mode::Hash(&source.hash),
            approve::Panes::default(),
            |_| Ok(None),
        ),
        Err(crate::Error::NotAutomation(_))
    ));

    // Dangling symlink inside the automations folder is skipped by scan::find and fails with Io.
    let dangling = paths.personal().join("dangling.luau");
    symlink(fixture.root().join("missing.luau"), &dangling).unwrap();
    assert_eq!(
        scan::find(paths, &store).unwrap(),
        vec![scan::Found {
            path: personal_script,
            repo: None,
        }]
    );
    assert!(matches!(
        approve::approve(
            paths,
            &store,
            &fixture.herdr,
            &dangling,
            Mode::Hash(&source.hash),
            approve::Panes::default(),
            |_| Ok(None),
        ),
        Err(crate::Error::Io { .. })
    ));
}
