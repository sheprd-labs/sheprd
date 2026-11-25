//! `phone`: agents driving a paired phone's app, and what it answered as
//! text and JSON.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::{PhoneCommand, PhoneTarget},
    control::{
        ErrorCode,
        protocol::{
            Request, Response,
            phone::{Answer, Ask},
        },
    },
};
use sheprd_remote_control::control::phone::{Command, Outcome, Phone, PhoneState, Refusal, Screen};

pub(super) fn phone(command: PhoneCommand, context: &Context) -> crate::Result<Output> {
    let (ask, asked) = match command {
        PhoneCommand::Targets => {
            return Output::data(
                format!(
                    "Pages: {}\nHighlights: {}",
                    sheprd_remote_control::control::phone::SettingsPage::ALL
                        .iter()
                        .map(|page| page.name())
                        .collect::<Vec<_>>()
                        .join(", "),
                    sheprd_remote_control::control::phone::Highlight::ALL
                        .iter()
                        .map(|target| target.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                &serde_json::json!({"pages": sheprd_remote_control::control::phone::SettingsPage::ALL, "highlights": sheprd_remote_control::control::phone::Highlight::ALL}),
            );
        }
        PhoneCommand::Guide {
            device,
            text,
            highlight,
        } => (
            drive(device, Command::Guide { text, highlight }),
            Asked::Show,
        ),
        PhoneCommand::Highlight { device, target } => {
            (drive(device, Command::Highlight { target }), Asked::Show)
        }
        PhoneCommand::ClearGuide { device } => (drive(device, Command::ClearGuide), Asked::Show),
        PhoneCommand::Phones => (Ask::Phones, Asked::Phones),
        PhoneCommand::State { device } => (drive(device, Command::State), Asked::State),
        PhoneCommand::Show { device, text } => (drive(device, Command::Show { text }), Asked::Show),
        PhoneCommand::Open { device, target } => {
            let command = open(target, context)?;
            (drive(device, command), Asked::Open)
        }
    };
    match context.call(Request::Phone, ask)? {
        Response::Phone { answer } => output(&answer, asked),
        response => Err(unexpected(response)),
    }
}

fn drive(device: Option<String>, command: Command) -> Ask {
    Ask::Drive { device, command }
}

/// What `open` names, with a pane or workspace left out taken from the
/// caller's environment.
fn open(target: PhoneTarget, context: &Context) -> crate::Result<Command> {
    let mine = |id: Option<String>, what: &str| {
        id.ok_or_else(|| crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            message: format!("Name a {what}: this command does not run in a Herdr pane"),
        })
    };
    Ok(match target {
        PhoneTarget::Screen(to) => Command::Open { to },
        PhoneTarget::Pane(pane) => Command::OpenPane {
            pane: match pane {
                Some(pane) => pane,
                None => mine(context.caller()?.pane_id, "pane")?,
            },
        },
        PhoneTarget::Workspace(workspace) => Command::Open {
            to: Screen::Workspace {
                workspace: match workspace {
                    Some(workspace) => workspace,
                    None => mine(context.caller()?.workspace_id, "workspace")?,
                },
            },
        },
        PhoneTarget::Changes(workspace) => Command::Open {
            to: Screen::Changes {
                workspace: match workspace {
                    Some(workspace) => workspace,
                    None => mine(context.caller()?.workspace_id, "workspace")?,
                },
            },
        },
    })
}

/// What the caller asked, which words the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Asked {
    Phones,
    State,
    Open,
    Show,
}

const NO_PHONE: &str = "No phone that lets agents open screens is connected: the Sheprd app must be open on it, with Let agents open screens on for this computer";

/// The answer as text and JSON, exiting as it says: no phone is nothing
/// yet, a refusal a failure.
pub(super) fn output(answer: &Answer, asked: Asked) -> crate::Result<Output> {
    let (phone, outcome) = match answer {
        Answer::Phones { phones } if phones.is_empty() => {
            return Ok(Output::data(NO_PHONE, answer)?.nothing());
        }
        Answer::Phones { phones } => {
            let lines: Vec<String> = phones
                .iter()
                .map(|phone| format!("{}\t{}", phone.name, phone.device.0))
                .collect();
            return Output::data(lines.join("\n"), answer);
        }
        Answer::NoPhone => return Ok(Output::data(NO_PHONE, answer)?.nothing()),
        Answer::Driven { phone, outcome } => (phone, outcome),
        other => {
            return Err(unexpected(Response::Phone {
                answer: other.clone(),
            }));
        }
    };
    match outcome {
        Outcome::Done { state } => Output::data(done(phone, state, asked), answer),
        Outcome::Refused { reason } => Ok(Output::data(refused(phone, *reason), answer)?.failed()),
    }
}

fn done(phone: &Phone, state: &PhoneState, asked: Asked) -> String {
    let name = &phone.name;
    match asked {
        Asked::Show => format!("Shown on {name}"),
        _ if state.locked => format!("{name} is locked"),
        Asked::Open => match &state.screen {
            Some(screen) => format!("{name} shows {}", label(screen)),
            None => format!("Opened on {name}"),
        },
        Asked::State | Asked::Phones => match &state.screen {
            Some(screen) => format!("{name} shows {}", label(screen)),
            None => format!("{name} shows another computer's screen, or a form"),
        },
    }
}

fn refused(phone: &Phone, reason: Refusal) -> String {
    let name = &phone.name;
    match reason {
        Refusal::Locked => format!("{name} is locked; its person unlocks the app first"),
        Refusal::NotFound => format!("{name} does not list that yet; try again in a moment"),
        Refusal::Off => format!("{name} no longer lets agents on this computer open screens"),
        Refusal::Other => format!("{name} refused"),
    }
}

/// A screen as a person names it.
pub(super) fn label(screen: &Screen) -> String {
    match screen {
        Screen::Home => "Home".into(),
        Screen::Workspaces => "Workspaces".into(),
        Screen::PullRequests => "Pull Requests".into(),
        Screen::SettingsPage { page } => format!("Settings > {}", page.name()),
        Screen::Settings => "Settings".into(),
        Screen::NewWorkspace => "the new workspace form".into(),
        Screen::Chat { pane } => format!("chat {pane}"),
        Screen::Term { pane } => format!("terminal {pane}"),
        Screen::Workspace { workspace } => format!("workspace {workspace}"),
        Screen::Changes { workspace } => format!("the changes of workspace {workspace}"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
