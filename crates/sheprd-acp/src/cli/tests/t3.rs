use super::*;

#[test]
fn t3_follows_a_thread_with_a_token_file() {
    let parsed = parse(
        [
            "t3",
            "mcp:abc",
            "--token-file",
            "/t",
            "--server",
            "http://127.0.0.1:3773",
            "--pane",
            "p1",
        ]
        .map(OsString::from),
    )
    .unwrap();
    assert_eq!(
        parsed,
        Parsed::T3(T3Options {
            endpoint: None,
            pane: Some("p1".to_owned()),
            server: Some("http://127.0.0.1:3773".to_owned()),
            thread: T3Thread::Existing("mcp:abc".to_owned()),
            token_file: PathBuf::from("/t"),
        })
    );
    assert_eq!(
        parse(["t3", "mcp:abc"].map(OsString::from)),
        Err(Error::MissingValue("--token-file"))
    );
    assert_eq!(
        parse(["t3"].map(OsString::from)),
        Err(Error::MissingValue("t3 THREAD"))
    );
}

#[test]
fn t3_new_starts_a_thread_before_following_it() {
    let parsed = parse(
        [
            "t3",
            "--new",
            "--cwd",
            "/repo",
            "--prompt",
            "-fix the build",
            "--title",
            "Build",
            "--provider",
            "codex",
            "--model",
            "gpt",
            "--mode",
            "approval-required",
            "--plan",
            "--base",
            "main",
            "--branch",
            "fix",
            "--token-file",
            "/t",
        ]
        .map(OsString::from),
    )
    .unwrap();
    let Parsed::T3(T3Options {
        thread: T3Thread::New(request),
        ..
    }) = parsed
    else {
        panic!("{parsed:?}");
    };
    assert_eq!(
        request,
        sheprd_t3::launch::Request {
            cwd: PathBuf::from("/repo"),
            interaction_mode: sheprd_t3::launch::InteractionMode::Plan,
            model: Some("gpt".to_owned()),
            prompt: Some("-fix the build".to_owned()),
            provider: Some("codex".to_owned()),
            runtime_mode: sheprd_t3::launch::RuntimeMode::ApprovalRequired,
            title: Some("Build".to_owned()),
            worktree: Some(sheprd_t3::launch::NewWorktree {
                base: "main".to_owned(),
                branch: Some("fix".to_owned()),
            }),
        }
    );
    let defaults = parse(["t3", "--new", "--token-file", "/t"].map(OsString::from)).unwrap();
    let Parsed::T3(T3Options {
        thread: T3Thread::New(request),
        ..
    }) = defaults
    else {
        panic!("{defaults:?}");
    };
    assert_eq!(request.cwd, PathBuf::from("."));
    assert_eq!(request.worktree, None);
    assert_eq!(
        parse(["t3", "--new", "--branch", "x", "--token-file", "/t"].map(OsString::from)),
        Err(Error::BranchWithoutBase)
    );
    assert!(matches!(
        parse(["t3", "--new", "--mode", "yolo", "--token-file", "/t"].map(OsString::from)),
        Err(Error::T3Mode(_))
    ));
    // Starting options name no existing thread.
    assert_eq!(
        parse(["t3", "mcp:abc", "--prompt", "x", "--token-file", "/t"].map(OsString::from)),
        Err(Error::UnknownOption(OsString::from("--prompt")))
    );
}
