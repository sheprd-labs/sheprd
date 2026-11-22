use super::*;
use control::{ChatCommand, ControlCommand, Invocation, Switch};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn command(args: &[&str]) -> ControlCommand {
    parse(args).unwrap().command
}

#[test]
fn chat_unread_toggles_unless_told() {
    assert_eq!(
        command(&["chat", "unread"]),
        ControlCommand::Chat(ChatCommand::Unread {
            switch: Switch::Toggle,
            pane: None
        })
    );
    assert_eq!(
        command(&["chat", "unread", "off", "--pane", "w1:p2"]),
        ControlCommand::Chat(ChatCommand::Unread {
            switch: Switch::Off,
            pane: Some("w1:p2".into())
        })
    );
    assert_eq!(
        command(&["chat", "unread", "--pane", "core:t7", "on"]),
        ControlCommand::Chat(ChatCommand::Unread {
            switch: Switch::On,
            pane: Some("core:t7".into())
        })
    );
    assert!(parse(&["chat", "unread", "maybe"]).is_err());
    assert!(parse(&["chat", "unread", "on", "extra"]).is_err());
}
