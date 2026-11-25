use super::*;
use control::{ControlCommand, Invocation, PhoneCommand, PhoneTarget};
use sheprd_remote_control::control::phone::Screen;

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn command(args: &[&str]) -> PhoneCommand {
    match parse(args).unwrap().command {
        ControlCommand::Phone(command) => command,
        other => panic!("{args:?} parsed as {other:?}"),
    }
}

fn opens(args: &[&str]) -> PhoneTarget {
    match command(args) {
        PhoneCommand::Open { target, .. } => target,
        other => panic!("{args:?} parsed as {other:?}"),
    }
}

#[test]
fn phone_lists_phones_by_default() {
    for args in [&["phone"][..], &["phone", "list"]] {
        assert_eq!(command(args), PhoneCommand::Phones);
    }
    let json = parse(&["phone", "state", "--json", "--device", "iPad"]).unwrap();
    assert!(json.json);
    assert_eq!(
        json.command,
        ControlCommand::Phone(PhoneCommand::State {
            device: Some("iPad".into())
        })
    );
}

#[test]
fn phone_open_names_a_tab_a_pane_or_a_workspace() {
    assert_eq!(opens(&["phone", "open"]), PhoneTarget::Pane(None));
    assert_eq!(
        opens(&["phone", "open", "w1-p2"]),
        PhoneTarget::Pane(Some("w1-p2".into()))
    );
    assert_eq!(
        opens(&["phone", "open", "settings"]),
        PhoneTarget::Screen(Screen::Settings)
    );
    assert_eq!(
        opens(&["phone", "open", "prs"]),
        PhoneTarget::Screen(Screen::PullRequests)
    );
    assert_eq!(
        opens(&["phone", "open", "term", "p3"]),
        PhoneTarget::Screen(Screen::Term { pane: "p3".into() })
    );
    assert_eq!(
        opens(&["phone", "open", "changes"]),
        PhoneTarget::Changes(None)
    );
    assert_eq!(
        opens(&["phone", "open", "workspace", "w2"]),
        PhoneTarget::Workspace(Some("w2".into()))
    );
    assert!(matches!(
        parse(&["phone", "open", "chat"]),
        Err(CliError::MissingArgument { .. })
    ));
}

#[test]
fn phone_show_takes_the_rest_as_its_note() {
    assert_eq!(
        command(&["phone", "show", "tests", "pass"]),
        PhoneCommand::Show {
            device: None,
            text: "tests pass".into()
        }
    );
    assert!(matches!(
        parse(&["phone", "show"]),
        Err(CliError::MissingArgument { .. })
    ));
    assert!(matches!(
        parse(&["phone", "show", "   "]),
        Err(CliError::InvalidValue(_))
    ));
    let long = "é".repeat(501);
    assert!(matches!(
        parse(&["phone", "show", &long]),
        Err(CliError::InvalidValue(_))
    ));
    assert!(parse(&["phone", "tap"]).is_err());
}

#[test]
fn phone_guidance_and_settings_targets() {
    use sheprd_remote_control::control::phone::{Highlight, SettingsPage};
    for page in SettingsPage::ALL {
        assert_eq!(
            opens(&["phone", "open", "settings", page.name()]),
            PhoneTarget::Screen(Screen::SettingsPage { page: *page })
        );
    }
    for target in Highlight::ALL {
        assert_eq!(
            command(&["phone", "highlight", target.name(), "--device", "one"]),
            PhoneCommand::Highlight {
                device: Some("one".into()),
                target: *target
            }
        );
    }
    assert_eq!(
        command(&[
            "phone",
            "guide",
            "Push settings",
            "--highlight",
            "push-relay"
        ]),
        PhoneCommand::Guide {
            device: None,
            text: "Push settings".into(),
            highlight: Some(Highlight::PushRelay)
        }
    );
    assert_eq!(
        command(&["phone", "guide", "--clear"]),
        PhoneCommand::ClearGuide { device: None }
    );
    for args in [
        vec!["phone", "guide", ""],
        vec!["phone", "guide", "--clear", "extra"],
        vec!["phone", "highlight", "unknown"],
        vec!["phone", "open", "settings", "unknown"],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
}

#[test]
fn desktop_guidance_preserves_window_target_and_rejects_extra_input() {
    use control::UiCommand;
    for (args, wanted) in [
        (
            vec!["ui", "settings", "notifications"],
            UiCommand::Settings(Some("notifications".into())),
        ),
        (
            vec!["ui", "highlight", "notif-sound"],
            UiCommand::Guide {
                text: None,
                highlight: Some("notif-sound".into()),
            },
        ),
        (
            vec![
                "ui",
                "guide",
                "Push",
                "settings",
                "--highlight",
                "remote-push-relay",
            ],
            UiCommand::Guide {
                text: Some("Push settings".into()),
                highlight: Some("remote-push-relay".into()),
            },
        ),
        (
            vec!["ui", "guide", "--clear"],
            UiCommand::Guide {
                text: None,
                highlight: None,
            },
        ),
    ] {
        let mut args = args;
        args.extend(["--window", "42", "--json"]);
        let found = parse(&args).unwrap();
        assert_eq!(found.command, ControlCommand::Ui(wanted));
        assert_eq!(found.window, Some(42));
        assert!(found.json);
    }
    for args in [
        vec!["ui", "settings", "one", "two"],
        vec!["ui", "guide", "--clear", "text"],
        vec!["ui", "guide", " "],
        vec!["ui", "highlight"],
    ] {
        assert!(parse(&args).is_err());
    }
}
