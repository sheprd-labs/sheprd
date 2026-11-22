//! `tab`, `code`, `devices`, `orchestrator`, and `review`.
use super::{Args, CliError, unknown};
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabCommand {
    List,
    Focus { id: u64 },
    Close { id: u64 },
}

/// `FILE[:LINE]`. A trailing `:N` is a line, as compilers and `grep -n`
/// print them; a file whose name really ends that way needs `./` and a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileAt {
    pub path: PathBuf,
    pub line: Option<u32>,
}

impl From<OsString> for FileAt {
    fn from(value: OsString) -> Self {
        let split = value.to_str().and_then(|text| {
            let (path, line) = text.rsplit_once(':')?;
            let line = line.parse::<u32>().ok().filter(|line| *line > 0)?;
            (!path.is_empty()).then(|| (PathBuf::from(path), line))
        });
        match split {
            Some((path, line)) => Self {
                path,
                line: Some(line),
            },
            None => Self {
                path: value.into(),
                line: None,
            },
        }
    }
}

/// `machine`: a virtual Mac on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachineCommand {
    Open {
        name: String,
    },
    /// Open it with its setup guide beside it.
    Setup {
        name: String,
    },
    List,
    Set {
        name: String,
        cpus: Option<u16>,
        memory_gib: Option<u16>,
        display: Option<String>,
    },
    Suspend {
        name: String,
    },
    Delete {
        name: String,
    },
    /// From an image, or without one from Apple's macOS, which the user
    /// confirms in Settings.
    New {
        name: Option<String>,
        image: Option<String>,
    },
    ImageList,
    ImageSave {
        machine: String,
        image: String,
    },
    ImageDelete {
        name: String,
    },
    /// What Sheprd Agent inside it reports.
    Agent {
        name: String,
    },
    /// A command inside it; `device` is its name.
    Exec(super::ExecCommand),
    Screenshot {
        name: String,
        path: PathBuf,
    },
    Click {
        name: String,
        x: u32,
        y: u32,
        button: super::MachineButton,
        count: u8,
    },
    Type {
        name: String,
        text: String,
    },
    Key {
        name: String,
        key: String,
        modifiers: Vec<super::MachineModifier>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeCommand {
    /// A read-only code tab beside the terminals.
    Open(FileAt),
    /// The user's terminal editor, in a new pane.
    Edit(FileAt),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Items {
    #[default]
    All,
    Issues,
    PullRequests,
    Tasks,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrchestratorCommand {
    Adopt {
        repo: Option<PathBuf>,
        run: String,
    },
    Restart {
        repo: Option<PathBuf>,
        run: String,
    },
    Open {
        repo: Option<PathBuf>,
    },
    /// The issues and pull requests the orchestrator last synced, read from
    /// its local database.
    Items {
        repo: Option<PathBuf>,
        items: Items,
    },
}

fn id(args: &mut Args) -> Result<u64, CliError> {
    args.required_text("ID")?
        .parse()
        .map_err(|_| CliError::InvalidValue("ID"))
}

pub(super) fn tab(args: &mut Args) -> Result<TabCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(TabCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => TabCommand::List,
        Some("focus") => TabCommand::Focus { id: id(args)? },
        Some("close") => TabCommand::Close { id: id(args)? },
        _ => return Err(unknown("tab", verb)),
    })
}

pub(super) fn code(args: &mut Args) -> Result<CodeCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "code",
        what: "open|edit",
    })?;
    Ok(match verb.to_str() {
        Some("open") => CodeCommand::Open(args.required("FILE[:LINE]")?.into()),
        Some("edit") => CodeCommand::Edit(args.required("FILE[:LINE]")?.into()),
        _ => return Err(unknown("code", verb)),
    })
}

/// A family whose only verb is `open`, which may be left out.
pub(super) fn open_only(args: &mut Args) -> Result<(), CliError> {
    match args.verb() {
        None => Ok(()),
        Some(verb) if verb == "open" => Ok(()),
        Some(verb) => Err(unknown(args.name(), verb)),
    }
}

pub(super) fn machine(args: &mut Args) -> Result<MachineCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(MachineCommand::Open {
            name: "default".to_owned(),
        });
    };
    Ok(match verb.to_str() {
        Some("open") => MachineCommand::Open {
            name: args
                .optional_text()?
                .unwrap_or_else(|| "default".to_owned()),
        },
        Some("setup") => MachineCommand::Setup {
            name: args
                .optional_text()?
                .unwrap_or_else(|| "default".to_owned()),
        },
        Some("list") => MachineCommand::List,
        Some("set") => MachineCommand::Set {
            cpus: args.number("--cpus", 1..=u16::MAX)?,
            memory_gib: args.number("--memory", 1..=u16::MAX)?,
            display: args.text_option("--display")?,
            name: args.required_text("NAME")?,
        },
        Some("suspend") => MachineCommand::Suspend {
            name: args.required_text("NAME")?,
        },
        Some("delete") => MachineCommand::Delete {
            name: args.required_text("NAME")?,
        },
        Some("new") => {
            let image = args.text_option("--from")?;
            let name = match image {
                Some(_) => Some(args.required_text("NAME")?),
                None => args.optional_text()?,
            };
            MachineCommand::New { name, image }
        }
        Some("image") => machine_image(args)?,
        Some(verb @ ("agent" | "exec" | "screenshot" | "click" | "type" | "key")) => {
            super::machine_agent::parse(verb, args)?
        }
        _ => return Err(unknown("machine", verb)),
    })
}

fn machine_image(args: &mut Args) -> Result<MachineCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(MachineCommand::ImageList);
    };
    Ok(match verb.to_str() {
        Some("list") => MachineCommand::ImageList,
        Some("save") => MachineCommand::ImageSave {
            machine: args.required_text("MACHINE")?,
            image: args.required_text("IMAGE")?,
        },
        Some("delete") => MachineCommand::ImageDelete {
            name: args.required_text("IMAGE")?,
        },
        _ => return Err(unknown("machine image", verb)),
    })
}

pub(super) fn orchestrator(args: &mut Args) -> Result<OrchestratorCommand, CliError> {
    // The option first, so `orchestrator --repo PATH` opens PATH.
    let repo = args.option("--repo")?.map(PathBuf::from);
    let verb = args.verb();
    Ok(match verb.as_ref().and_then(|verb| verb.to_str()) {
        None | Some("open") => OrchestratorCommand::Open { repo },
        Some("restart") => OrchestratorCommand::Restart {
            repo,
            run: args.required_text("RUN")?,
        },
        Some("adopt") => OrchestratorCommand::Adopt {
            repo,
            run: args.required_text("RUN")?,
        },
        Some("items") => OrchestratorCommand::Items {
            repo,
            items: Items::All,
        },
        Some("issues") => OrchestratorCommand::Items {
            repo,
            items: Items::Issues,
        },
        Some("prs") => OrchestratorCommand::Items {
            repo,
            items: Items::PullRequests,
        },
        Some("tasks") => OrchestratorCommand::Items {
            repo,
            items: Items::Tasks,
        },
        _ => return Err(unknown("orchestrator", verb.unwrap_or_default())),
    })
}
