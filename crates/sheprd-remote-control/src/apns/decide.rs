//! When a chat change starts, updates, or ends the chat's Live Activity, and
//! what it shows. Pure, with the inputs [`crate::push::decide`] takes: the
//! daemon passes each change with the transcript it applies to, before
//! applying it, and the time.
//!
//! | Change | Activity |
//! | --- | --- |
//! | a turn starts | start, working |
//! | a tool call starts or the plan moves, during a turn | update with the step |
//! | a permission is asked (not auto-accepted) | update, question, alert |
//! | the last waiting permission is resolved | update, working |
//! | the turn's summary arrives | end, finished (failed when it failed) |
//! | the turn ends without a summary, or the agent fails mid-turn | end, failed |
//!
//! A cancelled turn ends at once, since the user stopped it; any other ends
//! and stays on the lock screen for [`LINGER_S`].

use super::{
    Alert, Attributes, ContentState, Detail, Event, Live, Phase,
    activity::{self, Described},
    payload::{MAX_BUTTONS, MAX_PREVIOUS},
};
use crate::push::{Chat, excerpt};
use sheprd_chat_protocol::{
    Change, EntryKind, Permission, PlanEntry, PlanStatus, Status, Tool, ToolKind, ToolStatus,
    Transcript, TurnEnd, TurnSummary,
};

/// How long a finished activity stays on the lock screen.
pub(crate) const LINGER_S: u64 = 15 * 60;
/// The step shown before the agent names one.
pub(crate) const DEFAULT_STEP: &str = "Working";

/// The push `change` calls for, given the transcript `before` it applies to,
/// on the computer named `computer` whose CA has the SHA-256 `fingerprint`
/// (any case, colons allowed, as `state.fingerprint` writes it), at
/// `now_ms`.
pub(crate) fn decide(
    chat: &Chat<'_>,
    computer: &str,
    fingerprint: &str,
    before: &Transcript,
    change: &Change,
    now_ms: u64,
) -> Option<Live> {
    let now_s = now_ms / 1000;
    let running = before.turn.is_some();
    match change {
        Change::Turn { turn: Some(turn) } if !running => {
            let mut state = state(before, Phase::Working, Step::idle());
            state.started_at = turn.started_ms / 1000;
            // The plan and its counts belong to the turn before.
            state.plan_done = None;
            state.plan_total = None;
            Some(live(
                chat,
                Event::Start {
                    alert: Alert {
                        body: place(chat),
                        title: format!("{} is working", chat.agent),
                    },
                    attributes: Attributes {
                        agent: excerpt(chat.agent),
                        chat_title: excerpt(chat.title),
                        computer: excerpt(computer),
                        computer_id: computer_id(fingerprint),
                        pane: chat.pane.to_owned(),
                        terminal: false,
                        workspace: excerpt(chat.workspace),
                    },
                },
                state,
            ))
        }
        Change::Turn { turn: None } if running && !summarized(before) => {
            let mut state = state(before, Phase::Failed, now(before, &before.plan));
            state.summary = Some(format!("{} stopped", chat.agent));
            Some(end(chat, state, now_s + LINGER_S))
        }
        Change::Push { entry } if running => match &entry.kind {
            EntryKind::Turn(summary) => Some(turn_ended(chat, before, summary, now_s)),
            EntryKind::Tool(tool) => tool_step(chat, before, tool),
            _ => None,
        },
        Change::Replace { entry } if running => match &entry.kind {
            EntryKind::Tool(tool) => tool_step(chat, before, tool),
            _ => None,
        },
        Change::Plan { plan } if running => {
            let mut state = state(before, phase(before), now(before, plan));
            (state.plan_done, state.plan_total) = plan_counts(plan);
            let unchanged = state.step == current_step(before)
                && (state.plan_done, state.plan_total) == plan_counts(&before.plan);
            (!unchanged).then(|| update(chat, state, None))
        }
        Change::PermissionAsked { permission } if running && !before.auto_accept => {
            let mut state = state(before, Phase::Question, now(before, &before.plan));
            state.question = Some(excerpt(&permission.title));
            asking(&mut state, permission);
            let alert = Alert {
                body: place(chat),
                title: format!("{} needs an answer", chat.agent),
            };
            Some(update(chat, state, Some(alert)))
        }
        Change::PermissionResolved { id } if running => {
            let last = matches!(before.permissions.as_slice(), [only] if only.id == *id);
            last.then(|| {
                update(
                    chat,
                    state(before, Phase::Working, now(before, &before.plan)),
                    None,
                )
            })
        }
        Change::Status {
            status: Status::Failed { message },
        } if running => {
            let mut state = state(before, Phase::Failed, now(before, &before.plan));
            state.summary = Some(excerpt(message));
            Some(end(chat, state, now_s + LINGER_S))
        }
        _ => None,
    }
}

fn turn_ended(chat: &Chat<'_>, before: &Transcript, summary: &TurnSummary, now_s: u64) -> Live {
    let phase = match summary.end {
        TurnEnd::Failed | TurnEnd::Refused => Phase::Failed,
        TurnEnd::Done | TurnEnd::Cancelled | TurnEnd::Limit | TurnEnd::Other => Phase::Finished,
    };
    let mut state = state(before, phase, now(before, &before.plan));
    state.summary = Some(summary.describe());
    let dismissal_s = if summary.end == TurnEnd::Cancelled {
        now_s
    } else {
        now_s + LINGER_S
    };
    end(chat, state, dismissal_s)
}

/// An update naming `tool` as the step, when it is running and is not
/// already the step shown.
fn tool_step(chat: &Chat<'_>, before: &Transcript, tool: &Tool) -> Option<Live> {
    let running = matches!(tool.status, ToolStatus::Pending | ToolStatus::InProgress);
    if !running || tool.title.trim().is_empty() {
        return None;
    }
    let step = Step::of(tool);
    let new = !turn_tools(before).any(|known| known.call_id == tool.call_id);
    // A new call counts even when its step reads like the one shown, as a
    // command run again does: its counts changed.
    if step.step == current_step(before) && !new {
        return None;
    }
    let mut state = state(before, phase(before), step);
    if new {
        state.detail.steps = Some(state.detail.steps.unwrap_or(0).saturating_add(1));
    }
    Some(update(chat, state, None))
}

/// The step an activity shows: the tool call's title with a shell wrapper
/// taken off, or a plan step, and, for the phone's styles, it in words.
struct Step {
    step: String,
    described: Option<Described>,
}

impl Step {
    fn idle() -> Self {
        Self {
            step: DEFAULT_STEP.to_owned(),
            described: None,
        }
    }

    fn of(tool: &Tool) -> Self {
        Self {
            step: excerpt(&activity::clean(&tool.title)),
            described: Some(activity::describe(tool)),
        }
    }
}

/// The current turn's tool calls with a title, newest first.
fn turn_tools(transcript: &Transcript) -> impl Iterator<Item = &Tool> {
    transcript
        .entries
        .iter()
        .rev()
        .take_while(|entry| !matches!(entry.kind, EntryKind::User { .. }))
        .filter_map(|entry| match &entry.kind {
            EntryKind::Tool(tool) if !tool.title.trim().is_empty() => Some(tool),
            _ => None,
        })
}

/// The phase a turn is in apart from the change at hand.
fn phase(before: &Transcript) -> Phase {
    if before.is_blocked() {
        Phase::Question
    } else {
        Phase::Working
    }
}

/// The current turn as an update, including its permission buttons.
pub(crate) fn current(before: &Transcript) -> ContentState {
    state(before, phase(before), now(before, &before.plan))
}

/// The content shared by every push for a running turn.
fn state(before: &Transcript, phase: Phase, step: Step) -> ContentState {
    let (plan_done, plan_total) = plan_counts(&before.plan);
    let earlier = previous(before, &step.step);
    let count = |n: usize| (n > 0).then(|| u32::try_from(n).unwrap_or(u32::MAX));
    let mut edited: Vec<&str> = turn_tools(before)
        .filter(|tool| {
            matches!(
                tool.kind,
                ToolKind::Edit | ToolKind::Delete | ToolKind::Move
            )
        })
        .flat_map(|tool| tool.locations.iter().map(String::as_str))
        .collect();
    edited.sort_unstable();
    edited.dedup();
    let detail = Detail {
        focus: None,
        question_revision: None,
        agents: None,
        activity: step
            .described
            .as_ref()
            .map(|described| described.activity.clone()),
        files: count(edited.len()),
        icon: step.described.as_ref().map(|described| described.icon),
        previous_activities: earlier
            .iter()
            .map(|tool| activity::describe(tool).activity)
            .collect(),
        steps: count(turn_tools(before).count()),
    };
    let mut state = ContentState {
        detail,
        phase,
        plan_done,
        plan_total,
        options: Vec::new(),
        permission: None,
        previous: earlier
            .iter()
            .map(|tool| excerpt(&activity::clean(&tool.title)))
            .collect(),
        question: None,
        started_at: before.turn.map_or(0, |turn| turn.started_ms / 1000),
        step: step.step,
        summary: None,
        waiting: 0,
        working: 0,
    };
    if phase == Phase::Question
        && let Some(permission) = before.permissions.first()
    {
        state.question = Some(excerpt(&permission.title));
        asking(&mut state, permission);
    }
    state
}

/// The answer buttons for `permission`.
fn asking(state: &mut ContentState, permission: &Permission) {
    state.permission = Some(permission.id.0);
    state.options = permission
        .options
        .iter()
        .take(MAX_BUTTONS)
        .cloned()
        .collect();
}

/// The current turn's last [`MAX_PREVIOUS`] tool calls whose step is not
/// `step`, each step once, oldest first: the stack the lock screen shows
/// above it.
fn previous<'a>(transcript: &'a Transcript, step: &str) -> Vec<&'a Tool> {
    let mut seen: Vec<String> = Vec::new();
    let mut tools: Vec<&Tool> = Vec::new();
    for tool in turn_tools(transcript) {
        let title = excerpt(&activity::clean(&tool.title));
        if title != step && !seen.contains(&title) {
            seen.push(title);
            tools.push(tool);
        }
        if tools.len() == MAX_PREVIOUS {
            break;
        }
    }
    tools.reverse();
    tools
}

fn plan_counts(plan: &[PlanEntry]) -> (Option<u32>, Option<u32>) {
    if plan.is_empty() {
        return (None, None);
    }
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let done = plan
        .iter()
        .filter(|entry| entry.status == PlanStatus::Completed)
        .count();
    (Some(count(done)), Some(count(plan.len())))
}

/// What the turn is doing now: the running tool call of the current turn,
/// else the plan's step in progress, else the turn's last tool call.
pub(crate) fn current_step(transcript: &Transcript) -> String {
    now(transcript, &transcript.plan).step
}

/// [`current_step`] of `transcript`'s entries with `plan` as its plan, and
/// it in words.
fn now(transcript: &Transcript, plan: &[PlanEntry]) -> Step {
    let mut last_tool = None;
    for tool in turn_tools(transcript) {
        if matches!(tool.status, ToolStatus::Pending | ToolStatus::InProgress) {
            return Step::of(tool);
        }
        last_tool.get_or_insert(tool);
    }
    let planned = plan
        .iter()
        .find(|entry| entry.status == PlanStatus::InProgress);
    match (planned, last_tool) {
        (Some(entry), _) => Step {
            step: excerpt(&entry.content),
            described: Some(activity::planned(&entry.content)),
        },
        (None, Some(tool)) => Step::of(tool),
        (None, None) => Step::idle(),
    }
}

/// Whether the transcript's last entry is a turn summary, so a following
/// turn end needs no push of its own.
fn summarized(transcript: &Transcript) -> bool {
    transcript
        .entries
        .last()
        .is_some_and(|entry| matches!(entry.kind, EntryKind::Turn(_)))
}

/// The activity's `computerId`: the CA fingerprint as 64 uppercase hex
/// digits without separators, the form the pairing QR carries and the app
/// stores (`Fingerprint.hex`).
pub(crate) fn computer_id(fingerprint: &str) -> String {
    fingerprint
        .chars()
        .filter(|c| *c != ':')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn place(chat: &Chat<'_>) -> String {
    if chat.title.is_empty() {
        excerpt(chat.workspace)
    } else {
        excerpt(&format!("{} \u{b7} {}", chat.workspace, chat.title))
    }
}

fn live(chat: &Chat<'_>, event: Event, state: ContentState) -> Live {
    Live {
        event,
        pane: chat.pane.to_owned(),
        state,
    }
}

fn update(chat: &Chat<'_>, state: ContentState, alert: Option<Alert>) -> Live {
    live(chat, Event::Update { alert }, state)
}

fn end(chat: &Chat<'_>, state: ContentState, dismissal_s: u64) -> Live {
    live(chat, Event::End { dismissal_s }, state)
}
