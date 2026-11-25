use super::*;
use sheprd_t3::launch::RuntimeMode;

#[test]
fn new_takes_where_and_how_to_start() {
    let new = parse(args(&[
        "new",
        "--token-file",
        "/t",
        "--cwd",
        "/repo",
        "--prompt",
        "hi",
        "--mode",
        "auto",
        "--base",
        "main",
    ]))
    .unwrap();
    let Command::New(request) = new.command else {
        panic!("{new:?}");
    };
    assert_eq!(request.cwd, PathBuf::from("/repo"));
    assert_eq!(request.prompt.as_deref(), Some("hi"));
    assert_eq!(request.runtime_mode, RuntimeMode::Auto);
    assert_eq!(
        request.worktree,
        Some(NewWorktree {
            base: "main".to_owned(),
            branch: None,
        })
    );
    let here = parse(args(&["new", "--token-file", "/t"])).unwrap();
    let Command::New(request) = here.command else {
        panic!("{here:?}");
    };
    assert_eq!(request.cwd, PathBuf::from("."));
}

#[test]
fn starting_options_belong_to_new_alone() {
    assert_eq!(
        parse(args(&["threads", "--prompt", "hi", "--token-file", "/t"])),
        Err(Error::NewOnly("--prompt"))
    );
    assert_eq!(
        parse(args(&["new", "--branch", "x", "--token-file", "/t"])),
        Err(Error::BranchWithoutBase)
    );
    assert!(matches!(
        parse(args(&["new", "--mode", "yolo", "--token-file", "/t"])),
        Err(Error::Mode(_))
    ));
    assert_eq!(
        parse(args(&["new", "extra", "--token-file", "/t"])),
        Err(Error::Arguments("new"))
    );
}
