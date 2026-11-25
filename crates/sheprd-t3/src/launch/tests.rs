use super::*;
use crate::fixtures::{LAUNCHES, STEER, frames};

mod recorded;

fn fake_provider(id: &str, models: &[(&str, bool)]) -> Provider {
    serde_json::from_value(json!({"instanceId": id, "models": models
        .iter()
        .map(|(slug, default)| json!({"slug": slug, "name": slug, "isDefault": default}))
        .collect::<Vec<_>>()}))
    .unwrap()
}

fn project(root: &str, default: Option<(&str, &str)>) -> Project {
    Project {
        default_model_selection: default.map(|(instance, model)| ModelSelection {
            instance_id: instance.to_owned(),
            model: model.to_owned(),
        }),
        id: "project-1".to_owned(),
        title: "repo".to_owned(),
        workspace_root: root.to_owned(),
    }
}

#[test]
fn a_folder_outside_git_is_its_own_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    assert_eq!(
        checkout(dir.path()).unwrap(),
        Checkout {
            root,
            worktree: None
        }
    );
}

#[test]
fn a_folder_in_a_repository_starts_at_its_root() {
    let dir = tempfile::tempdir().unwrap();
    let repo = fs::canonicalize(dir.path()).unwrap().join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("src/deep")).unwrap();
    assert_eq!(
        checkout(&repo.join("src/deep")).unwrap(),
        Checkout {
            root: repo,
            worktree: None
        }
    );
}

#[test]
fn a_linked_worktree_belongs_to_its_repositorys_project() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let repo = base.join("repo");
    let admin = repo.join(".git/worktrees/feature");
    fs::create_dir_all(&admin).unwrap();
    fs::write(admin.join("commondir"), "../..\n").unwrap();
    let worktree = base.join("repo-feature");
    fs::create_dir_all(worktree.join("src")).unwrap();
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    assert_eq!(
        checkout(&worktree.join("src")).unwrap(),
        Checkout {
            root: repo.clone(),
            worktree: Some(worktree.clone()),
        }
    );
    // A submodule's `.git` file leads to a Git folder with no `commondir`:
    // the submodule is its own root.
    let modules = repo.join(".git/modules/lib");
    fs::create_dir_all(&modules).unwrap();
    let submodule = repo.join("lib");
    fs::create_dir_all(&submodule).unwrap();
    fs::write(submodule.join(".git"), "gitdir: ../.git/modules/lib\n").unwrap();
    assert_eq!(
        checkout(&submodule).unwrap(),
        Checkout {
            root: submodule,
            worktree: None,
        }
    );
    // A `.git` file that leads nowhere is not guessed at.
    fs::write(worktree.join(".git"), "gitdir: /nowhere\n").unwrap();
    assert!(matches!(checkout(&worktree), Err(Error::Worktree { .. })));
}

#[test]
fn a_missing_folder_is_an_error_naming_it() {
    let Err(Error::Checkout { path, .. }) = checkout(Path::new("/no/such/folder")) else {
        panic!("an error");
    };
    assert_eq!(path, Path::new("/no/such/folder"));
}

#[test]
fn the_project_for_a_root_is_found_however_its_path_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let mut shell = Shell::default();
    // The temporary folder's own name may go through a link, such as
    // macOS's /var to /private/var.
    let written = dir.path().join(".").to_string_lossy().into_owned();
    shell
        .projects
        .insert("p".to_owned(), project(&written, None));
    assert_eq!(project_for(&shell, &root).unwrap().id, "project-1");
    assert!(project_for(&shell, Path::new("/elsewhere")).is_none());
}

#[test]
fn the_model_is_the_one_asked_for_else_the_projects_else_the_first_agents() {
    let providers = [
        fake_provider("a", &[("a1", false), ("a2", true)]),
        fake_provider("b", &[("b1", false)]),
    ];
    let pick = |request: &Request, project: Option<&Project>| {
        model_for(request, project, &providers)
            .map(|selection| format!("{}/{}", selection.instance_id, selection.model))
    };
    let mut request = Request::default();
    assert_eq!(pick(&request, None).unwrap(), "a/a2");
    let preferred = project("/r", Some(("b", "b1")));
    assert_eq!(pick(&request, Some(&preferred)).unwrap(), "b/b1");
    // A project's default for an agent T3 no longer runs is passed over.
    let gone = project("/r", Some(("c", "c1")));
    assert_eq!(pick(&request, Some(&gone)).unwrap(), "a/a2");
    // A model alone keeps the project's agent.
    let both = [
        fake_provider("a", &[("shared", true)]),
        fake_provider("b", &[("shared", false), ("b2", true)]),
    ];
    let model_only = Request {
        model: Some("shared".to_owned()),
        ..Request::default()
    };
    let selection = model_for(&model_only, Some(&preferred), &both).unwrap();
    assert_eq!(selection.instance_id, "b");
    request.provider = Some("b".to_owned());
    assert_eq!(pick(&request, Some(&preferred)).unwrap(), "b/b1");
    request.model = Some("a1".to_owned());
    assert!(matches!(
        pick(&request, None),
        Err(Error::UnknownAgent(name)) if name == "b/a1"
    ));
    request.provider = Some("z".to_owned());
    assert!(matches!(pick(&request, None), Err(Error::UnknownAgent(_))));
    assert!(matches!(
        model_for(&Request::default(), None, &[]),
        Err(Error::NoAgent)
    ));
}

#[test]
fn a_prompt_names_the_thread_until_t3_does() {
    let checkout = Checkout {
        root: PathBuf::from("/r"),
        worktree: None,
    };
    let selection = ModelSelection::default();
    let request = Request {
        prompt: Some(format!("  {}\nmore", "x".repeat(100))),
        ..Request::default()
    };
    let launched = launch(&request, &checkout, "p", selection.clone(), Uuid::nil());
    assert_eq!(launched.title, "x".repeat(TITLE_CHARS));
    assert!(launched.generate_title);
    let blank = Request {
        prompt: Some("   ".to_owned()),
        ..Request::default()
    };
    let launched = launch(&blank, &checkout, "p", selection, Uuid::nil());
    assert_eq!(launched.title, "New thread");
    assert!(!launched.generate_title);
    assert_eq!(launched.initial_message, None);
}

#[test]
fn modes_parse_from_t3s_own_words() {
    for mode in [
        "approval-required",
        "auto-accept-edits",
        "auto",
        "full-access",
    ] {
        let parsed: RuntimeMode = mode.parse().unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), mode);
    }
    assert_eq!("yolo".parse::<RuntimeMode>(), Err(UnknownMode));
}
