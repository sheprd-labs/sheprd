use super::*;
use crate::traces::{Command, DEFAULT_DAYS, DEFAULT_LIMIT};
use sheprd_client::herdr::AgentKind;

#[test]
fn traces_commands_and_their_options() {
    assert_eq!(
        parse(args(&["traces", "exchange"])),
        Ok(Parsed::Traces(Command::Exchange))
    );
    assert_eq!(
        parse(args(&["traces", "peers", "--json"])),
        Ok(Parsed::Traces(Command::Peers { json: true }))
    );
    assert_eq!(
        parse(args(&["traces", "index"])),
        Ok(Parsed::Traces(Command::Index { json: false }))
    );
    assert_eq!(
        parse(args(&[
            "traces", "list", "--agent", "codex", "--limit", "5", "--json"
        ])),
        Ok(Parsed::Traces(Command::List {
            agent: Some(AgentKind::Codex),
            limit: 5,
            json: true,
        }))
    );
    assert_eq!(
        parse(args(&["traces", "list"])),
        Ok(Parsed::Traces(Command::List {
            agent: None,
            limit: DEFAULT_LIMIT,
            json: false,
        }))
    );
    assert_eq!(
        parse(args(&["traces", "activity", "--days", "7"])),
        Ok(Parsed::Traces(Command::Activity {
            days: 7,
            json: false
        }))
    );
    assert_eq!(
        parse(args(&["traces", "activity"])),
        Ok(Parsed::Traces(Command::Activity {
            days: DEFAULT_DAYS,
            json: false
        }))
    );
    assert_eq!(
        parse(args(&[
            "traces", "overlap", "--agent", "codex", "--host", "box", "--days", "7", "--at", "42",
            "--json"
        ])),
        Ok(Parsed::Traces(Command::Overlap {
            days: 7,
            agent: Some(AgentKind::Codex),
            host: Some("box".into()),
            at: Some(42),
            json: true
        }))
    );
    assert_eq!(parse(args(&["traces", "--help"])), Ok(Parsed::Help));
}

#[test]
fn traces_mistakes_are_named() {
    assert_eq!(
        parse(args(&["traces", "exchange", "--json"])),
        Err(Error::UnknownOption("--json".into()))
    );
    assert_eq!(
        parse(args(&["traces"])),
        Err(Error::MissingValue("traces command"))
    );
    assert_eq!(
        parse(args(&["traces", "share"])),
        Err(Error::UnknownOption("share".into()))
    );
    // Options belong to their own command.
    assert_eq!(
        parse(args(&["traces", "index", "--days", "3"])),
        Err(Error::UnknownOption("--days".into()))
    );
    assert_eq!(
        parse(args(&["traces", "list", "--agent", "nope"])),
        Err(Error::UnknownAgent("nope".into()))
    );
    assert_eq!(
        parse(args(&["traces", "list", "--limit", "many"])),
        Err(Error::MissingValue("--limit"))
    );
}
