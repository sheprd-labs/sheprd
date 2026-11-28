use super::*;

#[test]
fn builtin_agents_offer_switching_by_default() {
    assert!(run(&["claude"]).choose);
    assert!(run(&["--brief-file", "/tmp/brief.md", "claude"]).choose);
    assert!(run(&["--load", "s1", "claude"]).choose);
    assert!(run(&["--resume", "s1", "claude"]).choose);
    assert!(run(&["--resume", "s1", "claude", "--", "--model", "opus"]).choose);
    assert!(run(&["antigravity"]).choose);

    let options = run(&["--choose", "--headless", "codex"]);
    assert!(options.choose);
    assert_eq!(
        options.agent,
        AgentChoice::Builtin {
            agent: Agent::Codex,
            args: Vec::new(),
        }
    );
    assert!(run(&["--choose", "--load", "s1", "claude"]).choose);
    assert!(run(&["--choose", "--resume", "s1", "claude"]).choose);
    assert!(run(&["--choose", "claude", "--", "--model", "opus"]).choose);
    let antigravity = run(&["--choose", "antigravity"]);
    assert!(antigravity.choose);
    assert_eq!(
        antigravity.agent,
        AgentChoice::Builtin {
            agent: Agent::Antigravity,
            args: Vec::new(),
        }
    );
}

#[test]
fn custom_commands_stay_fixed_and_refuse_choose() {
    assert!(!run(&["--", "goose", "acp"]).choose);
    assert_eq!(
        parse(args(&["--choose", "--", "goose", "acp"])),
        Err(Error::ChooseCommand)
    );
    assert_eq!(parse(args(&["--choose"])), Err(Error::MissingAgent));
}
