//! `machine agent|exec|screenshot|click|type|key`: work inside a virtual Mac
//! through Sheprd Agent there.
use super::{Args, CliError, MachineButton, MachineCommand, MachineModifier, exec};

pub(super) fn parse(verb: &str, args: &mut Args) -> Result<MachineCommand, CliError> {
    Ok(match verb {
        "agent" => MachineCommand::Agent {
            name: args.required_text("NAME")?,
        },
        "exec" => MachineCommand::Exec(exec::parse_for(args, "machine exec", "NAME")?),
        "screenshot" => MachineCommand::Screenshot {
            name: args.required_text("NAME")?,
            path: args.required("PATH")?.into(),
        },
        "click" => {
            let button = if args.flag("--right") {
                MachineButton::Right
            } else {
                MachineButton::Left
            };
            let count = if args.flag("--double") { 2 } else { 1 };
            MachineCommand::Click {
                name: args.required_text("NAME")?,
                x: coordinate(args, "X")?,
                y: coordinate(args, "Y")?,
                button,
                count,
            }
        }
        "type" => MachineCommand::Type {
            name: args.required_text("NAME")?,
            text: args.required_text("TEXT")?,
        },
        _ => {
            let name = args.required_text("NAME")?;
            let (key, modifiers) = combination(&args.required_text("KEY")?)?;
            MachineCommand::Key {
                name,
                key,
                modifiers,
            }
        }
    })
}

fn coordinate(args: &mut Args, what: &'static str) -> Result<u32, CliError> {
    args.required_text(what)?
        .parse()
        .map_err(|_| CliError::InvalidValue(what))
}

/// A key and the modifiers held with it, as `cmd+shift+4` spells them.
pub(super) fn combination(text: &str) -> Result<(String, Vec<MachineModifier>), CliError> {
    let text = text.to_lowercase();
    let mut parts: Vec<&str> = text.split('+').collect();
    let key = parts.pop().filter(|key| !key.is_empty());
    let Some(key) = key else {
        return Err(CliError::InvalidValue("KEY"));
    };
    let modifiers = parts
        .into_iter()
        .map(|part| MachineModifier::parse(part).ok_or(CliError::InvalidValue("KEY")))
        .collect::<Result<_, _>>()?;
    Ok((key.to_owned(), modifiers))
}
