//! A T3 thread as a Sheprd chat transcript, so every chat view can show it.
//!
//! Prompts, replies, reasoning, and plans become text entries; commands,
//! tools, file changes, and searches become tool calls; notices, errors,
//! requests, and model switches become notices. Kinds with nothing to show,
//! such as checkpoints, are left out.

use sheprd_chat_protocol::{
    Entry, EntryKind, NoticeLevel, Permission, PermissionId, PermissionKind, PermissionOption,
    Question, QuestionOption, Status, Tool, ToolContent, ToolKind, ToolStatus, Transcript,
};

use crate::thread::{ApprovalOption, InputQuestion, Thread, TurnItem, TurnKind};

/// The decisions T3 takes for an approval, offered when a request names
/// none of its own (`providerPolicy.ts:50`).
const DECISIONS: [(&str, &str); 4] = [
    ("accept", "Allow"),
    ("acceptForSession", "Allow for this session"),
    ("decline", "Decline"),
    ("cancel", "Cancel the turn"),
];

/// The most of a command's output an entry keeps.
pub const MAX_OUTPUT: usize = 16 * 1024;

/// `thread` as a transcript, named for `agent`.
pub fn transcript(thread: &Thread, agent: &str) -> Transcript {
    let mut transcript = Transcript::new(agent);
    transcript.entries = thread
        .items()
        .into_iter()
        .filter_map(|(id, item)| {
            Some(Entry {
                id,
                kind: entry(thread, item)?,
            })
        })
        .collect();
    transcript.permissions = permissions(thread);
    transcript.status = if thread.working() {
        Status::Working
    } else {
        Status::Idle
    };
    transcript
}

fn entry(thread: &Thread, item: &TurnItem) -> Option<EntryKind> {
    let kind = match &item.kind {
        TurnKind::UserMessage { message_id, text } => EntryKind::User {
            guest: None,
            text: text_of(thread, message_id, text.as_deref()),
            files: 0,
            note: None,
        },
        TurnKind::AssistantMessage { message_id, text } => EntryKind::Agent {
            text: text_of(thread, message_id, text.as_deref()),
        },
        TurnKind::Reasoning { text } => EntryKind::Thought { text: text.clone() },
        TurnKind::ProposedPlan { markdown } => EntryKind::Agent {
            text: markdown.clone(),
        },
        TurnKind::CommandExecution {
            input,
            output,
            exit_code,
        } => {
            let mut content = Vec::new();
            if let Some(output) = output.as_deref().filter(|output| !output.is_empty()) {
                content.push(ToolContent::Text {
                    text: bounded(output),
                });
            }
            let title = item.title.clone().unwrap_or_else(|| input.clone());
            let mut tool = tool(item, ToolKind::Execute, title);
            tool.content = content;
            if exit_code.is_some_and(|code| code != 0) {
                tool.status = ToolStatus::Failed;
            }
            EntryKind::Tool(tool)
        }
        TurnKind::DynamicTool { tool_name, input } => {
            let title = item
                .title
                .clone()
                .or_else(|| tool_name.clone())
                .unwrap_or_else(|| "Tool".to_owned());
            let mut tool = tool(item, ToolKind::Other, title);
            if let Some(path) = input.get("path").and_then(|path| path.as_str()) {
                tool.locations.push(path.to_owned());
            }
            EntryKind::Tool(tool)
        }
        TurnKind::FileChange { file_name } => {
            let title = item
                .title
                .clone()
                .unwrap_or_else(|| format!("Edit {file_name}"));
            let mut tool = tool(item, ToolKind::Edit, title);
            tool.locations.push(file_name.clone());
            EntryKind::Tool(tool)
        }
        TurnKind::WebSearch { patterns } => {
            let title = item.title.clone().unwrap_or_else(|| {
                patterns
                    .as_ref()
                    .map_or_else(|| "Web search".to_owned(), |patterns| patterns.join(", "))
            });
            EntryKind::Tool(tool(item, ToolKind::Search, title))
        }
        TurnKind::ApprovalRequest { prompt, reason, .. } => notice(
            NoticeLevel::Warning,
            prompt
                .clone()
                .or_else(|| reason.clone())
                .or_else(|| item.title.clone())
                .unwrap_or_else(|| "The agent asks for approval.".to_owned()),
        ),
        TurnKind::UserInputRequest { questions, .. } => notice(
            NoticeLevel::Warning,
            questions
                .first()
                .map(|question| question.question.clone())
                .or_else(|| item.title.clone())
                .unwrap_or_else(|| "The agent asks a question.".to_owned()),
        ),
        TurnKind::SystemNotice { message } => notice(NoticeLevel::Info, message.clone()),
        TurnKind::Error { failure } => notice(
            NoticeLevel::Error,
            failure
                .get("message")
                .and_then(|message| message.as_str())
                .map_or_else(|| "The run failed.".to_owned(), str::to_owned),
        ),
        TurnKind::Handoff { to_model } => notice(
            NoticeLevel::Info,
            to_model.as_deref().map_or_else(
                || "The thread moved to another agent.".to_owned(),
                |model| format!("The thread moved to {model}."),
            ),
        ),
        TurnKind::Other => return None,
    };
    Some(kind)
}

/// The requests still waiting for an answer, oldest first. A request's
/// permission id is its entry's, so it stays the same while it waits.
///
/// Only a request a chat can answer becomes a permission: an approval, or
/// questions asked one at a time. Several questions at once stay a notice,
/// answered in T3 Code.
pub fn permissions(thread: &Thread) -> Vec<Permission> {
    thread
        .items()
        .into_iter()
        .filter_map(|(id, item)| {
            let permission = PermissionId(id.0);
            match &item.kind {
                TurnKind::ApprovalRequest {
                    options,
                    prompt,
                    reason,
                    request_id: Some(request),
                } if thread.pending(request) => Some(Permission {
                    guest: None,
                    call_id: None,
                    id: permission,
                    options: approval_options(options),
                    question: None,
                    title: prompt
                        .clone()
                        .or_else(|| reason.clone())
                        .or_else(|| item.title.clone())
                        .unwrap_or_else(|| "The agent asks for approval.".to_owned()),
                }),
                TurnKind::UserInputRequest {
                    questions,
                    request_id: Some(request),
                } if thread.pending(request) => match questions.as_slice() {
                    [question] => Some(Permission {
                        guest: None,
                        call_id: None,
                        id: permission,
                        options: Vec::new(),
                        question: Some(self::question(question)),
                        title: question.question.clone(),
                    }),
                    _ => None,
                },
                _ => None,
            }
        })
        .collect()
}

/// The request's own options, or every decision T3 takes.
pub(crate) fn approval_options(options: &[ApprovalOption]) -> Vec<PermissionOption> {
    if options.is_empty() {
        return DECISIONS
            .iter()
            .map(|(decision, label)| PermissionOption {
                id: (*decision).to_owned(),
                kind: decision_kind(decision),
                name: (*label).to_owned(),
            })
            .collect();
    }
    options
        .iter()
        .map(|option| PermissionOption {
            id: option.decision.clone(),
            kind: decision_kind(&option.decision),
            name: option.label.clone(),
        })
        .collect()
}

fn decision_kind(decision: &str) -> PermissionKind {
    match decision {
        "accept" => PermissionKind::AllowOnce,
        "acceptForSession" | "acceptAlways" => PermissionKind::AllowAlways,
        "decline" => PermissionKind::RejectOnce,
        _ => PermissionKind::Other,
    }
}

pub(crate) fn question(question: &InputQuestion) -> Question {
    Question {
        header: question.header.clone(),
        multiple: question.multi_select,
        options: question
            .options
            .iter()
            .map(|option| QuestionOption {
                description: option.description.clone(),
                label: option.label.clone(),
            })
            .collect(),
        text: question.question.clone(),
    }
}

/// A turn item's text, or its message's when the item leaves it out.
fn text_of(thread: &Thread, message_id: &str, text: Option<&str>) -> String {
    text.map(str::to_owned)
        .or_else(|| {
            thread
                .message(message_id)
                .map(|message| message.text.clone())
        })
        .unwrap_or_default()
}

fn tool(item: &TurnItem, kind: ToolKind, title: String) -> Tool {
    Tool {
        call_id: item.id.clone(),
        content: Vec::new(),
        kind,
        locations: Vec::new(),
        parent: None,
        started_ms: None,
        status: match item.status.as_deref() {
            Some("completed") => ToolStatus::Completed,
            Some("failed" | "cancelled" | "interrupted") => ToolStatus::Failed,
            Some("running" | "waiting") => ToolStatus::InProgress,
            _ => ToolStatus::Pending,
        },
        title,
    }
}

fn notice(level: NoticeLevel, text: String) -> EntryKind {
    EntryKind::Notice {
        answers: Vec::new(),
        details: None,
        level,
        text,
    }
}

/// `text` cut to [`MAX_OUTPUT`] bytes on a character boundary.
fn bounded(text: &str) -> String {
    if text.len() <= MAX_OUTPUT {
        return text.to_owned();
    }
    let mut end = MAX_OUTPUT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[cut]", &text[..end])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
