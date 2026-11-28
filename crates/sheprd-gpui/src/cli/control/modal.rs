//! CLI parser for `sheprd modal ...`.

use super::{Args, CliError, unknown};
use sheprd_control_protocol::protocol::modal::{CueKind, ModalOp, ModalScope, RunbookStep};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModalCommand {
    Op(ModalOp),
}

pub(super) fn command(args: &mut Args) -> Result<ModalCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(ModalCommand::Op(ModalOp::List));
    };
    let op = match verb.to_str() {
        Some("list") => {
            args.finish()?;
            ModalOp::List
        }
        Some("clear") => {
            let all = args.flag("--all");
            let id = args.text_option("--id")?;
            args.finish()?;
            ModalOp::Clear { all, id }
        }
        Some("cue") => {
            let name = args.words()?.ok_or(CliError::MissingArgument {
                command: "modal cue",
                what: "CUE",
            })?;
            let cue = CueKind::parse(&name).ok_or(CliError::InvalidValue(
                "yubikey|decision|secret|countdown|done",
            ))?;
            ModalOp::Cue { cue }
        }
        Some("show" | "urgent") => {
            let default_scope = if verb == "urgent" {
                ModalScope::All
            } else {
                ModalScope::Window
            };
            let scope = match args.text_option("--scope")?.as_deref() {
                None => default_scope,
                Some("window") => ModalScope::Window,
                Some("all" | "global") => ModalScope::All,
                Some(_) => return Err(CliError::InvalidValue("window|all")),
            };
            let title = args
                .text_option("--title")?
                .ok_or(CliError::MissingArgument {
                    command: "modal show",
                    what: "--title TITLE",
                })?;
            let preset = args.text_option("--preset")?;
            let choices = args
                .text_option("--choices")?
                .map(|raw| {
                    raw.split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let quick_reply = args.flag("--quick-reply");
            let cue = match args.text_option("--cue")? {
                Some(raw) => Some(CueKind::parse(&raw).ok_or(CliError::InvalidValue(
                    "yubikey|decision|secret|countdown|done",
                ))?),
                None => None,
            };
            let body = args.words()?.unwrap_or_default();
            ModalOp::Show {
                scope,
                title,
                body,
                preset,
                choices,
                quick_reply,
                cue,
            }
        }
        Some("countdown") => {
            let scope = match args.text_option("--scope")?.as_deref() {
                None | Some("window") => ModalScope::Window,
                Some("all" | "global") => ModalScope::All,
                Some(_) => return Err(CliError::InvalidValue("window|all")),
            };
            let title = args
                .text_option("--title")?
                .ok_or(CliError::MissingArgument {
                    command: "modal countdown",
                    what: "--title TITLE",
                })?;
            let seconds = args.number("--seconds", 1..=300)?.unwrap_or(15) as u32;
            let body = args.words()?.unwrap_or_default();
            ModalOp::Countdown {
                scope,
                title,
                body,
                seconds,
            }
        }
        Some("otp") => {
            let title = args
                .text_option("--title")?
                .unwrap_or_else(|| "Enter 2FA code".to_owned());
            let body = args.words()?.unwrap_or_default();
            ModalOp::Otp { title, body }
        }
        Some("oauth") => {
            let title = args
                .text_option("--title")?
                .unwrap_or_else(|| "Complete sign-in".to_owned());
            let url = args
                .text_option("--url")?
                .ok_or(CliError::MissingArgument {
                    command: "modal oauth",
                    what: "--url URL",
                })?;
            let code = args.text_option("--code")?;
            let body = args.words()?.unwrap_or_default();
            ModalOp::Oauth {
                title,
                body,
                url,
                code,
            }
        }
        Some("secret") => {
            let key = args
                .text_option("--key")?
                .ok_or(CliError::MissingArgument {
                    command: "modal secret",
                    what: "--key NAME",
                })?;
            let dest = args
                .text_option("--dest")?
                .ok_or(CliError::MissingArgument {
                    command: "modal secret",
                    what: "--dest env-file:PATH|keychain:NAME",
                })?;
            let title = args
                .text_option("--title")?
                .unwrap_or_else(|| format!("Provide {key}"));
            let reason = args
                .text_option("--reason")?
                .or(args.words()?)
                .unwrap_or_default();
            let save_keychain = args.flag("--save-keychain");
            ModalOp::Secret {
                title,
                key,
                dest,
                reason,
                save_keychain,
            }
        }
        Some("progress") => {
            let title = args
                .text_option("--title")?
                .ok_or(CliError::MissingArgument {
                    command: "modal progress",
                    what: "--title TITLE",
                })?;
            let percent = args.number("--percent", 0..=100)?.map(|n| n as u8);
            let step = args.text_option("--step")?.unwrap_or_default();
            let done = args.flag("--done");
            args.finish()?;
            ModalOp::Progress {
                title,
                percent,
                step,
                done,
            }
        }
        Some("runbook") => {
            let title = args
                .text_option("--title")?
                .ok_or(CliError::MissingArgument {
                    command: "modal runbook",
                    what: "--title TITLE",
                })?;
            let raw = args.words()?.unwrap_or_default();
            let steps = raw
                .split('|')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|entry| {
                    let (human, label) = if let Some(rest) = entry.strip_prefix("agent:") {
                        (false, rest.trim())
                    } else if let Some(rest) = entry.strip_prefix("you:") {
                        (true, rest.trim())
                    } else {
                        (true, entry)
                    };
                    RunbookStep {
                        label: label.to_owned(),
                        human,
                        done: false,
                    }
                })
                .collect();
            ModalOp::Runbook { title, steps }
        }
        Some("drop") => {
            let scope = match args.text_option("--scope")?.as_deref() {
                None | Some("all" | "global") => ModalScope::All,
                Some("window") => ModalScope::Window,
                Some(_) => return Err(CliError::InvalidValue("window|all")),
            };
            let title = args
                .text_option("--title")?
                .unwrap_or_else(|| "Drop a file for the agent".to_owned());
            let accept = args.text_option("--accept")?.unwrap_or_default();
            args.finish()?;
            ModalOp::Drop {
                scope,
                title,
                accept,
            }
        }
        Some("diff") => {
            let title = args
                .text_option("--title")?
                .unwrap_or_else(|| "Approve change?".to_owned());
            let file = args.text_option("--file")?.unwrap_or_default();
            let diff = args.words()?.unwrap_or_default();
            ModalOp::Diff { title, file, diff }
        }
        Some("spotlight") => {
            let title = args
                .text_option("--title")?
                .ok_or(CliError::MissingArgument {
                    command: "modal spotlight",
                    what: "--title TITLE",
                })?;
            let target = args
                .text_option("--target")?
                .ok_or(CliError::MissingArgument {
                    command: "modal spotlight",
                    what: "--target TARGET",
                })?;
            let step = args.text_option("--step")?;
            let body = args.words()?.unwrap_or_default();
            ModalOp::Spotlight {
                title,
                target,
                body,
                step,
            }
        }
        _ => return Err(unknown("modal", verb)),
    };
    Ok(ModalCommand::Op(op))
}
