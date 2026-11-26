use super::*;

#[test]
fn machine_agent_operations_round_trip() {
    for op in [
        TabOp::MachineAgent { name: "a".into() },
        TabOp::MachineClaim { name: "a".into() },
        TabOp::MachineRelease { name: "a".into() },
        TabOp::MachineClick {
            name: "a".into(),
            x: 10,
            y: 20,
            button: MachineButton::Right,
            count: 2,
        },
        TabOp::MachineExec {
            name: "a".into(),
            command: vec!["ls".into(), "-la".into()],
            cwd: Some("~/code".into()),
            wait_seconds: 60,
        },
        TabOp::MachineKey {
            name: "a".into(),
            key: "q".into(),
            modifiers: vec![MachineModifier::Cmd, MachineModifier::Shift],
        },
        TabOp::MachineScreenshot {
            name: "a".into(),
            path: "/tmp/a.png".into(),
        },
        TabOp::MachineType {
            name: "a".into(),
            text: "hello".into(),
        },
    ] {
        round_trip(op);
    }
}

#[test]
fn a_click_is_left_unless_asked() {
    let op: TabOp =
        serde_json::from_str(r#"{"op":"machine_click","name":"a","x":1,"y":2,"count":1}"#).unwrap();
    assert!(matches!(
        op,
        TabOp::MachineClick {
            button: MachineButton::Left,
            ..
        }
    ));
}

#[test]
fn a_command_waits_for_its_own_timeout_and_input_for_the_agent() {
    let wait = |op: TabOp| {
        Request::Tab(Call {
            caller: Caller::default(),
            op,
        })
        .wait_seconds()
    };
    let exec = TabOp::MachineExec {
        name: "a".into(),
        command: vec!["true".into()],
        cwd: None,
        wait_seconds: 100,
    };
    assert!(wait(exec) > 100);
    let typed = TabOp::MachineType {
        name: "a".into(),
        text: "x".into(),
    };
    assert_eq!(
        wait(typed),
        MACHINE_AGENT_WAIT_SECONDS + MACHINE_TYPE_SECONDS
    );
    assert!(MACHINE_TYPE_SECONDS * 1000 >= MAX_MACHINE_TYPE_CHARS as u64 * MACHINE_TYPE_GAP_MS);
}

#[test]
fn modifiers_take_their_usual_spellings() {
    assert_eq!(MachineModifier::parse("cmd"), Some(MachineModifier::Cmd));
    assert_eq!(
        MachineModifier::parse("ctrl"),
        Some(MachineModifier::Control)
    );
    assert_eq!(MachineModifier::parse("alt"), Some(MachineModifier::Option));
    assert_eq!(MachineModifier::parse("hyper"), None);
    let wire = serde_json::to_string(&MachineModifier::Option).unwrap();
    assert_eq!(wire, r#""option""#);
}

#[test]
fn agent_replies_round_trip() {
    for reply in [
        Reply::MachineAgent(MachineAgentInfo {
            version: 1,
            width: 2560,
            height: 1600,
            scale: 2,
            accessibility: true,
            screen_recording: false,
            capabilities: None,
        }),
        Reply::MachineScreenshot {
            path: "/tmp/a.png".into(),
            width: 2560,
            height: 1600,
        },
        Reply::MachineExec(Box::new(ExecInfo {
            device: "a".into(),
            status: Some(0),
            stdout: "out".into(),
            stderr: String::new(),
            truncated: false,
        })),
    ] {
        let response = Response::Tab { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
}

#[test]
fn machine_setup_round_trips_and_waits_to_arrive() {
    let op = TabOp::MachineSetup { name: "a".into() };
    assert_eq!(op.wait_seconds(), ARRIVE_WAIT_SECONDS);
    round_trip(op);
}

#[test]
fn machine_reservation_listing_accepts_older_replies() {
    let mut info: MachineInfo = serde_json::from_str(r#"{"name":"one","state":"running","cpus":4,"memory_gib":8,"display":"fit","disk_bytes":0}"#).unwrap();
    assert_eq!(info.reservation, None);
    info.reservation = Some("busy: terminal t7".into());
    let wire = serde_json::to_string(&info).unwrap();
    assert_eq!(serde_json::from_str::<MachineInfo>(&wire).unwrap(), info);
}
