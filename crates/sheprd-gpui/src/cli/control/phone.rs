//! `phone`: agents driving a paired phone's app.
use crate::cli::control::{Args, CliError, unknown};
use sheprd_remote_control::control::phone::{self, Screen};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhoneCommand {
    /// The phones connected that let agents drive them.
    Phones,
    Targets,
    Guide {
        device: Option<String>,
        text: String,
        highlight: Option<phone::Highlight>,
    },
    Highlight {
        device: Option<String>,
        target: phone::Highlight,
    },
    ClearGuide {
        device: Option<String>,
    },
    /// Which screen the phone shows.
    State {
        device: Option<String>,
    },
    Open {
        device: Option<String>,
        target: PhoneTarget,
    },
    /// A note on the phone, from this computer's agent.
    Show {
        device: Option<String>,
        text: String,
    },
    /// A Luau screen's files, laid over its bundled ones on the phone; or,
    /// with `reset`, the bundled ones again.
    Luau {
        device: Option<String>,
        dir: PathBuf,
        /// The screen's name in the app (`pull-requests`); the folder's by
        /// default.
        screen: Option<String>,
        /// Push again each time a file changes, until stopped.
        watch: bool,
        reset: bool,
    },
    /// A picture of what the phone shows, written to `out`, else to the
    /// path printed.
    Snapshot {
        device: Option<String>,
        out: Option<PathBuf>,
    },
}

/// What `phone open` shows. A pane or workspace left out is the caller's,
/// filled in from its environment when the command runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhoneTarget {
    Screen(Screen),
    /// A pane as the phone shows it: its chat, else its terminal.
    Pane(Option<String>),
    Workspace(Option<String>),
    Changes(Option<String>),
}

pub(super) fn phone(args: &mut Args) -> Result<PhoneCommand, CliError> {
    let device = args.text_option("--device")?;
    let Some(verb) = args.verb() else {
        return Ok(PhoneCommand::Phones);
    };
    Ok(match verb.to_str() {
        Some("list" | "phones") => PhoneCommand::Phones,
        Some("targets") => PhoneCommand::Targets,
        Some("highlight") => PhoneCommand::Highlight {
            device,
            target: phone::Highlight::parse(&args.required_text("TARGET")?)
                .ok_or(CliError::InvalidValue("TARGET (see phone targets)"))?,
        },
        Some("guide") if args.flag("--clear") => PhoneCommand::ClearGuide { device },
        Some("guide") => {
            let highlight = args
                .text_option("--highlight")?
                .map(|target| {
                    phone::Highlight::parse(&target)
                        .ok_or(CliError::InvalidValue("TARGET (see phone targets)"))
                })
                .transpose()?;
            let text = args.words()?.ok_or(CliError::MissingArgument {
                command: "phone guide",
                what: "TEXT",
            })?;
            if !phone::valid_text(&text) {
                return Err(CliError::InvalidValue(
                    "TEXT: at most 500 characters, not blank",
                ));
            }
            PhoneCommand::Guide {
                device,
                text,
                highlight,
            }
        }
        Some("state") => PhoneCommand::State { device },
        Some("open") => PhoneCommand::Open {
            device,
            target: target(args)?,
        },
        // A note may be typed unquoted, as `phone show tests pass`.
        Some("show") => {
            let text = args.words()?.ok_or(CliError::MissingArgument {
                command: "phone show",
                what: "TEXT",
            })?;
            // Refused here, so a bad note is a usage error, not the phone's.
            if !phone::valid_text(&text) {
                return Err(CliError::InvalidValue(
                    "TEXT: at most 500 characters, not blank",
                ));
            }
            PhoneCommand::Show { device, text }
        }
        Some("luau") => {
            let screen = args.text_option("--screen")?;
            let watch = args.flag("--watch");
            let reset = args.flag("--reset");
            let dir = PathBuf::from(args.required("DIR")?);
            if let Some(screen) = &screen
                && !phone::valid_screen(screen)
            {
                return Err(CliError::InvalidValue(
                    "--screen: a-z, 0-9 and -, as the app's folder of the screen is named",
                ));
            }
            if watch && reset {
                return Err(CliError::InvalidValue(
                    "--watch and --reset: one or the other",
                ));
            }
            PhoneCommand::Luau {
                device,
                dir,
                screen,
                watch,
                reset,
            }
        }
        Some("snapshot" | "screenshot") => PhoneCommand::Snapshot {
            device,
            out: args.option("--out")?.map(PathBuf::from),
        },
        _ => return Err(unknown("phone", verb)),
    })
}

fn target(args: &mut Args) -> Result<PhoneTarget, CliError> {
    let Some(word) = args.optional_text()? else {
        return Ok(PhoneTarget::Pane(None));
    };
    let pane = |args: &mut Args, command| {
        args.optional_text()?.ok_or(CliError::MissingArgument {
            command,
            what: "PANE",
        })
    };
    Ok(match word.as_str() {
        "home" => PhoneTarget::Screen(Screen::Home),
        "workspaces" => PhoneTarget::Screen(Screen::Workspaces),
        "pull-requests" | "prs" => PhoneTarget::Screen(Screen::PullRequests),
        "settings" => PhoneTarget::Screen(match args.optional_text()? {
            None => Screen::Settings,
            Some(page) => Screen::SettingsPage {
                page: phone::SettingsPage::parse(&page)
                    .ok_or(CliError::InvalidValue("PAGE (see phone targets)"))?,
            },
        }),
        "new-workspace" => PhoneTarget::Screen(Screen::NewWorkspace),
        "chat" => PhoneTarget::Screen(Screen::Chat {
            pane: pane(args, "phone open chat")?,
        }),
        "term" | "terminal" => PhoneTarget::Screen(Screen::Term {
            pane: pane(args, "phone open term")?,
        }),
        "pane" => PhoneTarget::Pane(args.optional_text()?),
        "workspace" => PhoneTarget::Workspace(args.optional_text()?),
        "changes" => PhoneTarget::Changes(args.optional_text()?),
        // Anything else is a pane's id, as `phone open w1-p2`.
        _ => PhoneTarget::Pane(Some(word)),
    })
}
