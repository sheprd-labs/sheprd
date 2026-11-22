//! Tools that show, keep, or start something, each as its `sheprd`
//! command does: restoring a checkpoint, creating a worktree, fanning out,
//! and teleporting only open Sheprd's own dialog, where the person decides;
//! memory entries and pins, worktree notes, checkpoints taken, pages, and
//! port forwards change at once.

use super::{Effect, Spec, integer, string};
use crate::cli::control::{
    BrowserCommand, CheckpointCommand, CodeCommand, ControlCommand, FanOut, FileAt, MemoryAdd,
    MemoryCommand, NotesCommand, Placement, PortsCommand, WorktreeCommand,
};
use serde_json::json;
use sheprd_memory::{Kind, MAX_DETAILS, MAX_NAME, MAX_TEXT};

/// The longest prompt, note, address, or path, as the CLI bounds them.
const LONG_TEXT: usize = 16 * 1024;
const SHORT_TEXT: usize = 1024;
/// The most agents of one kind a fan-out starts.
const MAX_PER_AGENT: u64 = 8;

pub(super) const TOOLS: &[Spec] = &[
    // keep-sorted start
    Spec {
        name: "browser_navigate",
        title: "Load another address",
        description: "Load another web address in a page tab, by the id browser_list reports.",
        effect: Effect::Changes,
        properties: || {
            json!({
                "id": integer("The page tab's id.", 0, u64::MAX),
                "url": string("The address to load.", LONG_TEXT),
            })
        },
        required: &["id", "url"],
        build: |arguments| {
            Ok(ControlCommand::Browser(BrowserCommand::Navigate {
                id: arguments.required_number("id", 0..=u64::MAX)?,
                url: arguments.required_text("url", LONG_TEXT)?,
            }))
        },
    },
    Spec {
        name: "browser_open",
        title: "Show a page",
        description: "Show the user a web page or a local HTML file (a mockup, an explainer) in a Sheprd browser tab. A local file opened from a Herdr pane goes beside that terminal unless placement says otherwise. The user can annotate it; read their notes with browser_feedback.",
        effect: Effect::Changes,
        properties: || {
            json!({
                "target": string("A web address, or a local file's path.", LONG_TEXT),
                "placement": { "type": "string", "enum": ["tab", "beside"], "description": "A tab of the window, or beside the caller's terminal." },
                "focus": { "type": "boolean", "description": "Switch the window to the new tab." },
            })
        },
        required: &["target"],
        build: |arguments| {
            Ok(ControlCommand::Browser(BrowserCommand::Open {
                target: arguments.required_text("target", LONG_TEXT)?,
                placement: arguments.choice(
                    "placement",
                    "tab, beside",
                    |placement| match placement {
                        "tab" => Some(Placement::Tab),
                        "beside" => Some(Placement::Beside),
                        _ => None,
                    },
                )?,
                focus: arguments.flag("focus")?,
                workspace: None,
            }))
        },
    },
    Spec {
        name: "browser_reload",
        title: "Reload pages",
        description: "Reload the page tabs the caller opened, after changing the files they show.",
        effect: Effect::Changes,
        properties: super::none,
        required: &[],
        build: |_| Ok(ControlCommand::Browser(BrowserCommand::Reload)),
    },
    Spec {
        name: "checkpoint_create",
        title: "Take a checkpoint",
        description: "Snapshot the files of the checkout the MCP server runs in now, so they can be compared or restored later.",
        effect: Effect::Changes,
        properties: || json!({ "label": string("What the checkpoint is for.", SHORT_TEXT) }),
        required: &[],
        build: |arguments| {
            Ok(ControlCommand::Checkpoint(CheckpointCommand::Create {
                label: arguments.text("label", SHORT_TEXT)?,
            }))
        },
    },
    Spec {
        name: "checkpoint_restore",
        title: "Restore a checkpoint",
        description: "Open the caller's checkout's checkpoints dialog in Sheprd with a checkpoint chosen. Nothing changes until the user confirms the restore there.",
        effect: Effect::Removes,
        properties: || json!({ "id": string("The checkpoint, as checkpoint_list names it; absent, the newest.", SHORT_TEXT) }),
        required: &[],
        build: |arguments| {
            Ok(ControlCommand::Checkpoint(CheckpointCommand::Restore {
                id: arguments.text("id", SHORT_TEXT)?,
            }))
        },
    },
    Spec {
        name: "code_open",
        title: "Show a file",
        description: "Show a file at a line in a read-only Sheprd code tab beside the terminals.",
        effect: Effect::Changes,
        properties: || {
            json!({
                "path": string("The file's path.", LONG_TEXT),
                "line": integer("The line to show, from 1.", 1, u64::from(u32::MAX)),
            })
        },
        required: &["path"],
        build: |arguments| {
            let path = arguments.required_text("path", LONG_TEXT)?.into();
            let line = arguments
                .number("line", 1..=u64::from(u32::MAX))?
                .map(|line| u32::try_from(line).unwrap_or(u32::MAX));
            Ok(ControlCommand::Code(CodeCommand::Open(FileAt {
                path,
                line,
            })))
        },
    },
    Spec {
        name: "fan_out",
        title: "Fan a prompt out",
        description: "Open Sheprd's fan-out dialog prefilled to send one prompt to several agents, each in its own new worktree. The user starts it there.",
        effect: Effect::Changes,
        properties: || {
            json!({
                "prompt": string("What every agent is asked.", LONG_TEXT),
                "agents": {
                    "type": "array",
                    "maxItems": 16,
                    "description": "Which agents, and how many of each; absent, the dialog's own choice.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "kind": string("An agent such as claude, codex, or gemini.", 64),
                            "count": integer("How many of this agent (default 1).", 1, MAX_PER_AGENT),
                        },
                        "required": ["kind"],
                        "additionalProperties": false,
                    },
                },
            })
        },
        required: &["prompt"],
        build: |arguments| {
            Ok(ControlCommand::FanOut(FanOut {
                prompt: arguments.required_text("prompt", LONG_TEXT)?,
                agents: arguments.agents("agents", MAX_PER_AGENT)?,
            }))
        },
    },
    Spec {
        name: "memory_add",
        title: "Remember something",
        description: "Keep something the next agent in any worktree, tab, or chat of this repository would otherwise have to find out again: a command that works, a trap and its way around, a decision and its reason, or how the person wants things done. Secrets, tokens, and prompt markup are refused.",
        effect: Effect::Changes,
        properties: || {
            json!({
                "text": string("The entry, in a sentence or two.", MAX_TEXT),
                "kind": { "type": "string", "enum": ["fact", "gotcha", "command", "decision", "preference", "handoff"], "description": "What kind of entry (default fact)." },
                "name": string("A lasting name: a later add with it updates this entry.", MAX_NAME),
                "details": string("The longer story.", MAX_DETAILS),
                "branch": { "type": "boolean", "description": "Keep it for this branch's work only." },
                "pin": { "type": "boolean", "description": "Show it first to every agent." },
            })
        },
        required: &["text"],
        build: |arguments| {
            Ok(ControlCommand::Memory(MemoryCommand::Add(MemoryAdd {
                text: arguments.required_text("text", MAX_TEXT)?,
                kind: arguments
                    .choice(
                        "kind",
                        "fact, gotcha, command, decision, preference, handoff",
                        |kind| kind.parse::<Kind>().ok(),
                    )?
                    .unwrap_or_default(),
                name: arguments.text("name", MAX_NAME)?,
                details: arguments.text("details", MAX_DETAILS)?,
                branch: arguments.flag("branch")?,
                pinned: arguments.flag("pin")?,
            })))
        },
    },
    Spec {
        name: "memory_forget",
        title: "Forget a memory",
        description: "Remove an entry of the repository's agent memory that is wrong or no longer true.",
        effect: Effect::Removes,
        properties: || json!({ "id": integer("The entry's id.", 1, i64::MAX as u64) }),
        required: &["id"],
        build: |arguments| {
            Ok(ControlCommand::Memory(MemoryCommand::Forget {
                id: super::read::memory_id(arguments)?,
            }))
        },
    },
    Spec {
        name: "memory_pin",
        title: "Pin a memory",
        description: "Pin an entry so every agent sees it first (at most 50), or unpin it.",
        effect: Effect::Changes,
        properties: || {
            json!({
                "id": integer("The entry's id.", 1, i64::MAX as u64),
                "pinned": { "type": "boolean", "description": "false unpins it (default true)." },
            })
        },
        required: &["id"],
        build: |arguments| {
            let id = super::read::memory_id(arguments)?;
            let pinned = !matches!(arguments.optional_flag("pinned")?, Some(false));
            Ok(ControlCommand::Memory(MemoryCommand::Pin { id, pinned }))
        },
    },
    Spec {
        name: "notes_set",
        title: "Set the worktree note",
        description: "Replace the note on the caller's checkout, which Sheprd shows on its row.",
        effect: Effect::Changes,
        properties: || json!({ "text": string("The note, one line.", SHORT_TEXT) }),
        required: &["text"],
        build: |arguments| {
            Ok(ControlCommand::Notes(NotesCommand::Set(
                arguments.required_text("text", SHORT_TEXT)?,
            )))
        },
    },
    Spec {
        name: "ports_forward",
        title: "Forward a port",
        description: "Forward a port of the caller's SSH host to this computer.",
        effect: Effect::Changes,
        properties: || json!({ "port": integer("The port.", 1, u64::from(u16::MAX)) }),
        required: &["port"],
        build: |arguments| {
            Ok(ControlCommand::Ports(PortsCommand::Forward(port(
                arguments,
            )?)))
        },
    },
    Spec {
        name: "ports_open",
        title: "Open a port in a tab",
        description: "Show localhost:PORT in a Sheprd browser tab, forwarding the port first when the caller's host is remote.",
        effect: Effect::Changes,
        properties: || json!({ "port": integer("The port.", 1, u64::from(u16::MAX)) }),
        required: &["port"],
        build: |arguments| Ok(ControlCommand::Ports(PortsCommand::Open(port(arguments)?))),
    },
    Spec {
        name: "teleport",
        title: "Move a worktree",
        description: "Open Sheprd's teleport dialog to move the caller's own linked worktree, with its tabs and agents, to another host. Only the worktree of the Herdr pane the MCP client runs in can move. The user confirms there.",
        effect: Effect::Removes,
        properties: || {
            json!({
                "host": string("The host to move to, by id or label as the window lists it.", SHORT_TEXT),
            })
        },
        required: &["host"],
        build: |arguments| {
            Ok(ControlCommand::Teleport {
                host: arguments.required_text("host", SHORT_TEXT)?,
                worktree: None,
            })
        },
    },
    Spec {
        name: "worktree_create",
        title: "Create a worktree",
        description: "Open Sheprd's new-worktree dialog for the caller's workspace, with a branch filled in. The user confirms there.",
        effect: Effect::Changes,
        properties: || json!({ "branch": string("The new branch's name.", SHORT_TEXT) }),
        required: &[],
        build: |arguments| {
            Ok(ControlCommand::Worktree(WorktreeCommand::Create {
                branch: arguments.text("branch", SHORT_TEXT)?,
            }))
        },
    },
    // keep-sorted end
];

fn port(arguments: &mut super::Arguments) -> Result<u16, super::Error> {
    let port = arguments.required_number("port", 1..=u64::from(u16::MAX))?;
    Ok(u16::try_from(port).unwrap_or(u16::MAX))
}
