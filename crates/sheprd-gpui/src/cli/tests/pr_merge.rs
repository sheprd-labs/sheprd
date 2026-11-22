use super::*;
use sheprd_control_protocol::protocol::hosts::QueueOp;

fn command(args: &[&str]) -> control::ControlCommand {
    match LaunchOptions::parse(args.iter().copied()).unwrap().mode {
        LaunchMode::Control(invocation) => invocation.command,
        mode => panic!("not a control command: {mode:?}"),
    }
}

#[test]
fn merge_and_queue_parse_and_need_a_queue_change() {
    assert_eq!(
        command(&["pr", "merge", "--json"]),
        control::ControlCommand::Pr(control::PrCommand::Merge)
    );
    for (word, change) in [("add", QueueOp::Add), ("remove", QueueOp::Remove)] {
        assert_eq!(
            command(&["pr", "queue", word]),
            control::ControlCommand::Pr(control::PrCommand::Queue(change))
        );
    }
    assert!(LaunchOptions::parse(["pr", "queue"]).is_err());
    assert!(LaunchOptions::parse(["pr", "queue", "jump"]).is_err());
    assert!(LaunchOptions::parse(["pr", "merge", "extra"]).is_err());
}
