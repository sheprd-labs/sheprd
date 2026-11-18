use super::*;
use control::{ControlCommand, ExecCommand, MachineButton, MachineCommand, MachineModifier};

fn command(args: &[&str]) -> ControlCommand {
    match LaunchOptions::parse(args.iter().copied()).unwrap().mode {
        LaunchMode::Control(invocation) => invocation.command,
        mode => panic!("not a control command: {mode:?}"),
    }
}

fn machine(args: &[&str]) -> MachineCommand {
    match command(args) {
        ControlCommand::Machine(command) => command,
        command => panic!("not a machine command: {command:?}"),
    }
}

#[test]
fn machine_exec_takes_the_command_after_the_dashes() {
    assert_eq!(
        machine(&[
            "machine",
            "exec",
            "vm",
            "--timeout",
            "30",
            "--cwd",
            "~/app",
            "--",
            "make",
            "test"
        ]),
        MachineCommand::Exec(ExecCommand {
            device: "vm".to_owned(),
            command: vec!["make".to_owned(), "test".to_owned()],
            cwd: Some("~/app".to_owned()),
            timeout: 30,
        })
    );
}

#[test]
fn machine_exec_without_a_command_is_refused() {
    assert!(LaunchOptions::parse(["machine", "exec", "vm"]).is_err());
}

#[test]
fn machine_click_takes_a_point_and_a_button() {
    assert_eq!(
        machine(&["machine", "click", "vm", "120", "45"]),
        MachineCommand::Click {
            name: "vm".to_owned(),
            x: 120,
            y: 45,
            button: MachineButton::Left,
            count: 1,
        }
    );
    assert_eq!(
        machine(&["machine", "click", "--right", "--double", "vm", "1", "2"]),
        MachineCommand::Click {
            name: "vm".to_owned(),
            x: 1,
            y: 2,
            button: MachineButton::Right,
            count: 2,
        }
    );
    assert!(LaunchOptions::parse(["machine", "click", "vm", "-1", "2"]).is_err());
}

#[test]
fn machine_key_reads_a_combination() {
    assert_eq!(
        machine(&["machine", "key", "vm", "Cmd+Shift+4"]),
        MachineCommand::Key {
            name: "vm".to_owned(),
            key: "4".to_owned(),
            modifiers: vec![MachineModifier::Cmd, MachineModifier::Shift],
        }
    );
    assert_eq!(
        machine(&["machine", "key", "vm", "return"]),
        MachineCommand::Key {
            name: "vm".to_owned(),
            key: "return".to_owned(),
            modifiers: Vec::new(),
        }
    );
    for bad in ["cmd+", "hyper+a", ""] {
        assert!(
            LaunchOptions::parse(["machine", "key", "vm", bad]).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn machine_type_and_screenshot_and_agent_take_their_words() {
    assert_eq!(
        machine(&["machine", "type", "vm", "hello there"]),
        MachineCommand::Type {
            name: "vm".to_owned(),
            text: "hello there".to_owned(),
        }
    );
    assert_eq!(
        machine(&["machine", "screenshot", "vm", "shot.png"]),
        MachineCommand::Screenshot {
            name: "vm".to_owned(),
            path: "shot.png".into(),
        }
    );
    assert_eq!(
        machine(&["machine", "agent", "vm"]),
        MachineCommand::Agent {
            name: "vm".to_owned()
        }
    );
}
