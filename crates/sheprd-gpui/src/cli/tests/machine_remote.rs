use super::*;
use control::{ControlCommand, MachineCommand};

fn command(args: &[&str]) -> Result<ControlCommand, CliError> {
    match LaunchOptions::parse(args.iter().copied())?.mode {
        LaunchMode::Control(invocation) => Ok(invocation.command),
        mode => panic!("not a control command: {mode:?}"),
    }
}

#[test]
fn host_forwards_work_on_a_machine_wherever_it_is_written() {
    for args in [
        &["machine", "--host", "mini-2", "claim", "default"][..],
        &["machine", "claim", "default", "--host", "mini-2"],
    ] {
        assert_eq!(
            command(args).ok(),
            Some(ControlCommand::Machine(MachineCommand::On {
                host: "mini-2".to_owned(),
                command: Box::new(MachineCommand::Claim {
                    name: "default".to_owned(),
                    wait: 0,
                }),
            }))
        );
    }
}

#[test]
fn host_leaves_exec_words_alone() {
    let parsed = command(&[
        "machine", "exec", "default", "--host", "mini", "--", "ls", "--host",
    ]);
    let Ok(ControlCommand::Machine(MachineCommand::On { host, command })) = parsed else {
        panic!("not forwarded: {parsed:?}");
    };
    assert_eq!(host, "mini");
    let MachineCommand::Exec(exec) = *command else {
        panic!("not exec");
    };
    assert_eq!(exec.command, ["ls", "--host"]);
}

#[test]
fn host_refuses_making_deleting_and_changing_machines() {
    for args in [
        &["machine", "delete", "default", "--host", "mini"][..],
        &["machine", "new", "--host", "mini"],
        &["machine", "set", "default", "--cpus", "2", "--host", "mini"],
        &["machine", "image", "list", "--host", "mini"],
        &["machine", "lend", "--host", "mini"],
    ] {
        assert!(command(args).is_err(), "{args:?}");
    }
}

#[test]
fn start_and_lend_parse() {
    assert_eq!(
        command(&["machine", "start", "work"]).ok(),
        Some(ControlCommand::Machine(MachineCommand::Start {
            name: "work".to_owned()
        }))
    );
    assert_eq!(
        command(&["machine", "lend"]).ok(),
        Some(ControlCommand::Machine(MachineCommand::Lend))
    );
}
