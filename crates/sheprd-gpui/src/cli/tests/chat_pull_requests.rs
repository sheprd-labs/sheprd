use super::*;
use control::{ChatCommand, ChatPrCommand, ControlCommand, Invocation};

fn chat(args: &[&str]) -> Result<Invocation, CliError> {
    let mut all = vec!["chat", "pr"];
    all.extend_from_slice(args);
    match LaunchOptions::parse(all)?.mode {
        LaunchMode::Control(invocation) => Ok(invocation),
        mode => panic!("{mode:?}"),
    }
}

fn pr(command: ChatPrCommand) -> ControlCommand {
    ControlCommand::Chat(ChatCommand::PullRequests(command))
}

#[test]
fn chat_pr_lists_by_default_and_adds_an_address() {
    assert_eq!(chat(&[]).unwrap().command, pr(ChatPrCommand::List));
    assert_eq!(chat(&["list"]).unwrap().command, pr(ChatPrCommand::List));
    let url = "https://github.com/o/r/pull/7";
    let added = chat(&["add", url, "--json"]).unwrap();
    assert!(added.json);
    assert_eq!(added.command, pr(ChatPrCommand::Add { url: url.into() }));
}

#[test]
fn chat_pr_refuses_what_is_not_a_pull_request_before_running() {
    assert!(matches!(
        chat(&["add"]),
        Err(CliError::MissingArgument { .. })
    ));
    for bad in [
        "https://github.com/o/r/issues/7",
        "https://gitlab.com/o/r/pull/7",
        "o/r#7",
    ] {
        assert!(
            matches!(chat(&["add", bad]), Err(CliError::InvalidValue("URL"))),
            "{bad}"
        );
    }
    assert!(chat(&["merge", "7"]).is_err());
}
