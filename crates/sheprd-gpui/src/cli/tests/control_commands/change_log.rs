use super::*;
use control::ChangeCommand;

#[test]
fn change_log_takes_an_optional_commit_prefix() {
    for args in [&["change"][..], &["change", "log"]] {
        assert_eq!(
            command(args),
            ControlCommand::Change(ChangeCommand::Log { commit: None })
        );
    }
    assert_eq!(
        command(&["change", "log", "ABCD123", "--json"]),
        ControlCommand::Change(ChangeCommand::Log {
            commit: Some("abcd123".into())
        })
    );
    for bad in ["abc", "not-hex", &"a".repeat(65)] {
        assert_eq!(
            parse(&["change", "log", bad]).unwrap_err(),
            CliError::InvalidValue("COMMIT"),
            "{bad}"
        );
    }
    assert!(parse(&["change", "evolve"]).is_err());
}

#[test]
fn change_diff_takes_one_or_two_commits() {
    assert_eq!(
        command(&["change", "diff", "abcd"]),
        ControlCommand::Change(ChangeCommand::Diff {
            old: "abcd".into(),
            new: None
        })
    );
    assert_eq!(
        command(&["change", "diff", "abcd", "EF01"]),
        ControlCommand::Change(ChangeCommand::Diff {
            old: "abcd".into(),
            new: Some("ef01".into())
        })
    );
    assert!(parse(&["change", "diff"]).is_err());
}

#[test]
fn change_id_takes_nothing() {
    assert_eq!(
        command(&["change", "id"]),
        ControlCommand::Change(ChangeCommand::Id)
    );
    assert!(parse(&["change", "id", "extra"]).is_err());
}

#[test]
fn change_stale_takes_nothing() {
    assert_eq!(
        command(&["change", "stale"]),
        ControlCommand::Change(ChangeCommand::Stale)
    );
}
