//! Steering a thread: a chat's commands as T3 Code's
//! `orchestration.dispatchCommand` payloads.
//!
//! Each payload's `commandId` comes from the chat command's own id, so a
//! client sending the same command again reaches T3 with the same id, and
//! T3, which keeps a receipt per `commandId` (`Orchestrator.ts:11742`),
//! applies it once. Only the commands a chat sends are typed
//! (`orchestrationV2.ts:2870-3095`); anything T3 cannot take is refused here
//! with the chat's own [`Rejection`] before it reaches the server.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sheprd_chat_protocol::{
    Choice, Command, CommandId, ConfigOption, MAX_ATTACHMENTS, MAX_PROMPT_SIZE, OptionCategory,
    OptionKind, OptionValue, PermissionId, Rejection,
};

use crate::thread::{InputQuestion, ModelSelection, Thread, TurnKind};

/// The RPC that takes every command.
pub const DISPATCH: &str = "orchestration.dispatchCommand";

/// The setting that picks the agent T3 runs, by provider instance.
pub const PROVIDER: &str = "provider";
/// The setting that picks the provider's model.
pub const MODEL: &str = "model";

/// One `dispatchCommand` payload.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dispatch {
    pub command_id: String,
    pub thread_id: String,
    #[serde(flatten)]
    pub action: Action,
}

/// What a dispatch asks T3 to do.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum Action {
    /// A prompt (`orchestrationV2.ts:2870`), as T3's own client sends one
    /// typed in its composer.
    #[serde(rename = "message.dispatch", rename_all = "camelCase")]
    Message {
        /// T3 takes uploaded files only; a chat's files go in the text.
        attachments: [(); 0],
        created_by: Actor,
        creation_source: Source,
        dispatch_mode: DispatchMode,
        message_id: String,
        text: String,
    },
    /// An answer to an approval or to questions (`orchestrationV2.ts:2998`).
    #[serde(rename = "runtime-request.respond", rename_all = "camelCase")]
    Respond {
        #[serde(skip_serializing_if = "Option::is_none")]
        answers: Option<Map<String, Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision: Option<String>,
        request_id: String,
    },
    /// Stops the running turn (`orchestrationV2.ts:2948`), keeping queued
    /// prompts held as T3's own stop button does.
    #[serde(rename = "run.interrupt", rename_all = "camelCase")]
    Interrupt { hold_queue: bool, run_id: String },
    /// Another model of the thread's provider (`orchestrationV2.ts:2857`).
    #[serde(rename = "thread.model-selection.set", rename_all = "camelCase")]
    SetModel { model_selection: ModelSelection },
    /// Another provider, which T3 hands the thread to
    /// (`orchestrationV2.ts:3089`).
    #[serde(rename = "provider.switch", rename_all = "camelCase")]
    SwitchProvider { model_selection: ModelSelection },
}

/// Who a command comes from (`orchestrationV2.ts:76`): always the person.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    User,
}

/// Which kind of client sent it (`orchestrationV2.ts:79`). T3 has no kind
/// for other clients; `web` is what its own clients send by default.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Web,
}

/// When a prompt runs: now, or behind the turn under way, as a chat's
/// prompts do.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DispatchMode {
    StartImmediately,
    QueueAfterActive,
}

/// A provider instance a T3 server runs, from `server.getConfig`
/// (`server.ts:217`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub instance_id: String,
    #[serde(default)]
    pub models: Vec<Model>,
    /// `ready`, `warning`, `error`, or `disabled` (`server.ts:51`).
    #[serde(default)]
    pub status: Option<String>,
}

/// One of a provider's models (`server.ts:74`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[serde(default)]
    pub is_default: bool,
    pub name: String,
    pub slug: String,
}

fn yes() -> bool {
    true
}

/// The providers in a `server.getConfig` answer that can run a thread:
/// enabled, working, and with a model. One this version cannot read is
/// skipped.
pub fn providers(config: &Value) -> Vec<Provider> {
    config
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|provider| serde_json::from_value::<Provider>(provider.clone()).ok())
        .filter(|provider| {
            provider.enabled
                && !provider.models.is_empty()
                && !matches!(provider.status.as_deref(), Some("error" | "disabled"))
        })
        .collect()
}

/// The thread's provider and model as chat settings, when T3 named both.
///
/// The provider is a setting while another can take the thread, even when
/// the thread's own provider can no longer run; its models are a setting
/// while it can.
pub fn options(thread: &Thread, providers: &[Provider]) -> Vec<ConfigOption> {
    let Some(current) = &thread.header.model_selection else {
        return Vec::new();
    };
    let own = providers
        .iter()
        .find(|provider| provider.instance_id == current.instance_id);
    let mut options = Vec::new();
    if providers.len() > usize::from(own.is_some()) {
        options.push(ConfigOption {
            category: OptionCategory::Other,
            description: None,
            id: PROVIDER.to_owned(),
            kind: OptionKind::Select {
                choices: providers
                    .iter()
                    .map(|provider| choice(&provider.instance_id, provider_name(provider)))
                    .collect(),
                current: current.instance_id.clone(),
            },
            name: "Agent".to_owned(),
        });
    }
    let Some(own) = own else {
        return options;
    };
    let mut models: Vec<Choice> = own
        .models
        .iter()
        .map(|model| choice(&model.slug, &model.name))
        .collect();
    // A model the list no longer offers is still the one running.
    if !models.iter().any(|model| model.id == current.model) {
        models.insert(0, choice(&current.model, &current.model));
    }
    options.push(ConfigOption {
        category: OptionCategory::Model,
        description: None,
        id: MODEL.to_owned(),
        kind: OptionKind::Select {
            choices: models,
            current: current.model.clone(),
        },
        name: "Model".to_owned(),
    });
    options
}

fn provider_name(provider: &Provider) -> &str {
    provider
        .display_name
        .as_deref()
        .unwrap_or(&provider.instance_id)
}

fn choice(id: &str, name: &str) -> Choice {
    Choice {
        description: None,
        group: None,
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

/// The dispatch that carries the chat command `id` to `thread`, or why
/// T3 cannot take it.
pub fn dispatch(
    thread: &Thread,
    providers: &[Provider],
    id: CommandId,
    command: &Command,
) -> Result<Dispatch, Rejection> {
    let action = match command {
        Command::Prompt {
            text, attachments, ..
        } => {
            if text.trim().is_empty() && attachments.is_empty() {
                return Err(Rejection::EmptyPrompt);
            }
            if attachments.len() > MAX_ATTACHMENTS {
                return Err(Rejection::TooManyAttachments);
            }
            // The server runs on this computer, so its agent reads the files
            // where they are.
            let mut text = text.clone();
            for attachment in attachments {
                text.push_str("\n\n");
                text.push_str(&attachment.path);
            }
            if text.len() > MAX_PROMPT_SIZE {
                return Err(Rejection::PromptTooLong);
            }
            Action::Message {
                attachments: [],
                created_by: Actor::User,
                creation_source: Source::Web,
                dispatch_mode: if thread.active_run().is_some() {
                    DispatchMode::QueueAfterActive
                } else {
                    DispatchMode::StartImmediately
                },
                message_id: format!("sheprd:{}:message", id.0),
                text,
            }
        }
        Command::Cancel => Action::Interrupt {
            hold_queue: true,
            run_id: thread.active_run().ok_or(Rejection::NotWorking)?.to_owned(),
        },
        Command::Answer { option, permission } => {
            let (request, options) = approval(thread, *permission)?;
            if !options.iter().any(|offered| offered == option) {
                return Err(Rejection::UnknownOption);
            }
            Action::Respond {
                answers: None,
                decision: Some(option.clone()),
                request_id: request,
            }
        }
        Command::AnswerQuestion {
            answers,
            permission,
        } => {
            let (request, question) = asked(thread, *permission)?;
            Action::Respond {
                answers: Some(answer(question, answers)?),
                decision: None,
                request_id: request,
            }
        }
        Command::SetOption { id, value } => model_change(thread, providers, id, value)?,
        Command::SetMode { .. } => return Err(Rejection::UnknownMode),
        Command::Unqueue { .. } => return Err(Rejection::UnknownQueued),
        Command::StopActivity { .. } => return Err(Rejection::UnknownActivity),
        Command::SetAutoAccept { .. }
        | Command::SetAgent { .. }
        | Command::AddPullRequest { .. } => {
            return Err(Rejection::NotReady);
        }
    };
    Ok(Dispatch {
        command_id: format!("sheprd:{}", id.0),
        thread_id: thread.header.id.clone(),
        action,
    })
}

/// The approval `permission` stands for, if it still waits, and the
/// decisions it takes.
fn approval(thread: &Thread, permission: PermissionId) -> Result<(String, Vec<String>), Rejection> {
    let item = thread
        .item(sheprd_chat_protocol::EntryId(permission.0))
        .ok_or(Rejection::UnknownPermission)?;
    let TurnKind::ApprovalRequest {
        options,
        request_id: Some(request),
        ..
    } = &item.kind
    else {
        return Err(Rejection::UnknownPermission);
    };
    if !thread.pending(request) {
        return Err(Rejection::UnknownPermission);
    }
    let decisions = crate::transcript::approval_options(options)
        .into_iter()
        .map(|option| option.id)
        .collect();
    Ok((request.clone(), decisions))
}

/// The one question `permission` stands for, if it still waits.
fn asked(thread: &Thread, permission: PermissionId) -> Result<(String, &InputQuestion), Rejection> {
    let item = thread
        .item(sheprd_chat_protocol::EntryId(permission.0))
        .ok_or(Rejection::UnknownPermission)?;
    let TurnKind::UserInputRequest {
        questions,
        request_id: Some(request),
    } = &item.kind
    else {
        return Err(Rejection::UnknownPermission);
    };
    match questions.as_slice() {
        [question] if thread.pending(request) => Ok((request.clone(), question)),
        _ => Err(Rejection::UnknownPermission),
    }
}

/// The answers as T3 takes them: by question id, an option's value (or its
/// label) when one was picked, else the person's own words; a list when
/// the question takes several.
fn answer(question: &InputQuestion, answers: &[String]) -> Result<Map<String, Value>, Rejection> {
    let asked = crate::transcript::question(question);
    asked.check(answers)?;
    let mut values: Vec<Value> = answers
        .iter()
        .map(|answer| {
            question
                .options
                .iter()
                .find(|option| &option.label == answer)
                .map_or_else(
                    || answer.clone(),
                    |option| option.value.clone().unwrap_or_else(|| option.label.clone()),
                )
                .into()
        })
        .collect();
    let value = if question.multi_select {
        Value::Array(values)
    } else {
        values.pop().ok_or(Rejection::InvalidAnswer)?
    };
    Ok(Map::from_iter([(question.id.clone(), value)]))
}

/// A new provider or model: a model of the same provider is set in place;
/// another provider takes the thread with its default model, as T3's own
/// client decides (`shared/src/model.ts:17`).
fn model_change(
    thread: &Thread,
    providers: &[Provider],
    id: &str,
    value: &OptionValue,
) -> Result<Action, Rejection> {
    let OptionValue::Choice { id: choice } = value else {
        return Err(Rejection::UnknownSetting);
    };
    let accepted = options(thread, providers)
        .iter()
        .any(|option| option.id == id && option.accepts(value));
    let current = thread
        .header
        .model_selection
        .as_ref()
        .ok_or(Rejection::UnknownSetting)?;
    if !accepted {
        return Err(Rejection::UnknownSetting);
    }
    if id == MODEL {
        return Ok(Action::SetModel {
            model_selection: ModelSelection {
                instance_id: current.instance_id.clone(),
                model: choice.clone(),
            },
        });
    }
    let provider = providers
        .iter()
        .find(|provider| &provider.instance_id == choice)
        .ok_or(Rejection::UnknownSetting)?;
    let model = provider
        .models
        .iter()
        .find(|model| model.is_default)
        .or_else(|| provider.models.first())
        .ok_or(Rejection::UnknownSetting)?;
    let model_selection = ModelSelection {
        instance_id: provider.instance_id.clone(),
        model: model.slug.clone(),
    };
    Ok(if provider.instance_id == current.instance_id {
        Action::SetModel { model_selection }
    } else {
        Action::SwitchProvider { model_selection }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
