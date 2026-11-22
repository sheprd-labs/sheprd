use super::*;

fn command(args: &[&str]) -> control::ControlCommand {
    match LaunchOptions::parse(args.iter().copied()).unwrap().mode {
        LaunchMode::Control(invocation) => invocation.command,
        mode => panic!("not a control command: {mode:?}"),
    }
}

#[test]
fn machine_opens_the_default_without_a_name() {
    for args in [&["machine"][..], &["machine", "open"]] {
        assert_eq!(
            command(args),
            control::ControlCommand::Machine(control::MachineCommand::Open {
                name: "default".to_owned()
            })
        );
    }
}

#[test]
fn machine_open_takes_a_name() {
    assert_eq!(
        command(&["machine", "open", "work"]),
        control::ControlCommand::Machine(control::MachineCommand::Open {
            name: "work".to_owned()
        })
    );
}

#[test]
fn machine_set_takes_what_changes() {
    assert_eq!(
        command(&["machine", "set", "work", "--cpus", "6", "--display", "fit"]),
        control::ControlCommand::Machine(control::MachineCommand::Set {
            name: "work".to_owned(),
            cpus: Some(6),
            memory_gib: None,
            display: Some("fit".to_owned()),
        })
    );
}

#[test]
fn machine_list_suspend_and_delete_parse() {
    assert_eq!(
        command(&["machine", "list"]),
        control::ControlCommand::Machine(control::MachineCommand::List)
    );
    assert_eq!(
        command(&["machine", "suspend", "work"]),
        control::ControlCommand::Machine(control::MachineCommand::Suspend {
            name: "work".to_owned()
        })
    );
    assert_eq!(
        command(&["machine", "delete", "work"]),
        control::ControlCommand::Machine(control::MachineCommand::Delete {
            name: "work".to_owned()
        })
    );
}

#[test]
fn a_machine_verb_it_does_not_know_is_an_error() {
    assert!(LaunchOptions::parse(["machine", "explode"]).is_err());
    assert!(LaunchOptions::parse(["machine", "delete"]).is_err());
}

#[test]
fn machine_has_its_own_help() {
    assert_eq!(
        command(&["machine", "--help"]),
        control::ControlCommand::Help(Some("machine"))
    );
}

#[test]
fn machine_new_takes_an_image_or_asks_for_macos() {
    assert_eq!(
        command(&["machine", "new", "work", "--from", "base"]),
        control::ControlCommand::Machine(control::MachineCommand::New {
            name: Some("work".to_owned()),
            image: Some("base".to_owned()),
        })
    );
    // From an image, the new machine needs a name.
    assert!(LaunchOptions::parse(["machine", "new", "--from", "base"]).is_err());
    // Without one, Settings offers Apple's macOS, under a name or its own.
    assert_eq!(
        command(&["machine", "new", "work"]),
        control::ControlCommand::Machine(control::MachineCommand::New {
            name: Some("work".to_owned()),
            image: None,
        })
    );
    assert_eq!(
        command(&["machine", "new"]),
        control::ControlCommand::Machine(control::MachineCommand::New {
            name: None,
            image: None,
        })
    );
}

#[test]
fn machine_image_lists_saves_and_deletes() {
    for args in [&["machine", "image"][..], &["machine", "image", "list"]] {
        assert_eq!(
            command(args),
            control::ControlCommand::Machine(control::MachineCommand::ImageList)
        );
    }
    assert_eq!(
        command(&["machine", "image", "save", "work", "base"]),
        control::ControlCommand::Machine(control::MachineCommand::ImageSave {
            machine: "work".to_owned(),
            image: "base".to_owned(),
        })
    );
    assert_eq!(
        command(&["machine", "image", "delete", "base"]),
        control::ControlCommand::Machine(control::MachineCommand::ImageDelete {
            name: "base".to_owned(),
        })
    );
    assert!(LaunchOptions::parse(["machine", "image", "lsit"]).is_err());
}
