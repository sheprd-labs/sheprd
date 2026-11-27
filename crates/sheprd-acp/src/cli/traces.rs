use super::*;
use crate::traces::{Command, DEFAULT_DAYS, DEFAULT_LIMIT};
use sheprd_client::herdr::AgentKind;

/// Host indexing, queries, and the bounded replication exchange.
pub(super) fn parse(args: impl Iterator<Item = OsString>) -> Result<Parsed, Error> {
    let Some(mut args) = unless_help(args) else {
        return Ok(Parsed::Help);
    };
    let action = utf8(args.next(), "traces command")?;
    let (mut json, mut agent, mut limit, mut days) = (false, None, DEFAULT_LIMIT, DEFAULT_DAYS);
    let mut at = None;
    let mut host = None;
    while let Some(arg) = args.next() {
        match (action.as_str(), arg.to_str()) {
            ("index" | "watch" | "list" | "activity" | "overlap" | "peers", Some("--json")) => {
                json = true
            }
            ("list" | "overlap", Some("--agent")) => {
                let name = utf8(args.next(), "--agent")?;
                agent = Some(AgentKind::parse(&name).ok_or(Error::UnknownAgent(name))?);
            }
            ("list", Some("--limit")) => limit = number(args.next(), "--limit")?,
            ("overlap", Some("--host")) => host = Some(utf8(args.next(), "--host")?),
            ("overlap", Some("--at")) => at = Some(number(args.next(), "--at")?),
            ("activity" | "overlap", Some("--days")) => days = number(args.next(), "--days")?,
            _ => return Err(Error::UnknownOption(arg)),
        }
    }
    let command = match action.as_str() {
        "exchange" => Command::Exchange,
        "peers" => Command::Peers { json },
        "index" => Command::Index { json },
        "watch" => Command::Watch { json },
        "list" => Command::List { agent, limit, json },
        "overlap" => Command::Overlap {
            days,
            agent,
            host,
            at,
            json,
        },
        "activity" => Command::Activity { days, json },
        _ => return Err(Error::UnknownOption(action.into())),
    };
    Ok(Parsed::Traces(command))
}

fn number<T: std::str::FromStr>(value: Option<OsString>, option: &'static str) -> Result<T, Error> {
    utf8(value, option)?
        .parse()
        .map_err(|_| Error::MissingValue(option))
}
