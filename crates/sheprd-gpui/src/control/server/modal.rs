//! Control server handler for `sheprd modal ...` requests.

use crate::agent_interactions::{
    BlockDirective, EscalationLevel, InteractionStore, ParsedInteraction, Payload, SourceKey,
    UpsertInteraction, sync_hud_deferred,
};
use crate::control::protocol::{
    Call, ErrorCode, Response,
    modal::{MAX_BODY, MAX_TITLE, ModalOp, ModalScope, Reply},
};
use crate::window::main_window::MainWindow;
use gpui::App;
use std::time::Instant;

pub(super) fn serve(call: Call<ModalOp>, cx: &mut App) -> Response {
    let Call { caller, op } = call;
    let now = Instant::now();
    let store = InteractionStore::global(cx);

    match op {
        ModalOp::List => {
            if !super::valid_caller(&caller) {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    "Invalid workspace, pane, or daemon",
                );
            }
            let items = store.read(cx).summaries(now);
            Response::Modal {
                reply: Reply::Listed { items },
            }
        }
        ModalOp::Clear { all: true, .. } => {
            if !super::valid_caller(&caller) {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    "Invalid workspace, pane, or daemon",
                );
            }
            let count = store.update(cx, |s, cx| {
                let removed = s.clear_all();
                if removed > 0 {
                    cx.notify();
                }
                removed
            });
            for win in MainWindow::all(cx) {
                let _ = win.update(cx, |_, _, cx| cx.notify());
            }
            Response::Modal {
                reply: Reply::Cleared { count },
            }
        }
        ModalOp::Clear { all: false, id } => {
            if let Err(response) = super::window_for(&caller, cx) {
                return response;
            }
            let pane = caller.pane_id.as_deref().or(caller.terminal_id.as_deref());
            let count = store.update(cx, |s, cx| {
                let removed = if let Some(id) = id.as_deref() {
                    usize::from(s.dismiss(id))
                } else if let Some(pane) = pane {
                    s.clear_for_pane(pane)
                } else {
                    s.clear_all()
                };
                if removed > 0 {
                    cx.notify();
                }
                removed
            });
            for win in MainWindow::all(cx) {
                let _ = win.update(cx, |_, _, cx| cx.notify());
            }
            Response::Modal {
                reply: Reply::Cleared { count },
            }
        }
        ModalOp::Cue { cue } => {
            let target_window = match super::window_for(&caller, cx) {
                Ok(win) => win,
                Err(response) => return response,
            };
            let volume = target_window
                .read(cx)
                .map_or(80, |view| view.config.sound.volume);
            store.update(cx, |s, cx| {
                s.trigger_cue(cue, now, volume);
                cx.notify();
            });
            let _ = target_window.update(cx, |_, _, cx| cx.notify());
            Response::done(format!("Played {} cue", cue.name()))
        }
        other => {
            let target_window = match super::window_for(&caller, cx) {
                Ok(win) => win,
                Err(response) => return response,
            };
            let (window_id, volume, window_ok, global_ok, pill_ok, workspace_label, workspace_root) =
                target_window
                    .read(cx)
                    .map(|view| {
                        let pane_ref = caller.pane_id.as_deref().or(caller.terminal_id.as_deref());
                        let (label, root) = if let Some(pane) = pane_ref {
                            view.workspace_info_for_pane(pane)
                        } else if let Some(ws_id) = caller.workspace_id.as_deref() {
                            (ws_id.to_owned(), view.workspace_path_for_id(ws_id))
                        } else {
                            ("workspace".to_owned(), view.selected_workspace_path())
                        };
                        (
                            Some(target_window.window_id()),
                            view.config.sound.volume,
                            view.config.chat.agent_modals,
                            view.config.chat.global_modals,
                            view.config.chat.progress_pill,
                            label,
                            root,
                        )
                    })
                    .unwrap_or_else(|_| {
                        (
                            None,
                            80,
                            true,
                            true,
                            true,
                            caller
                                .workspace_id
                                .clone()
                                .unwrap_or_else(|| "workspace".to_owned()),
                            None,
                        )
                    });
            let source = SourceKey {
                window_id,
                pane_id: caller.pane_id.clone().or(caller.terminal_id.clone()),
            };
            let parsed = match into_parsed(other) {
                Ok(parsed) => parsed,
                Err(msg) => return Response::error(ErrorCode::InvalidRequest, msg),
            };
            let is_progress = matches!(parsed.payload, Payload::Progress { .. });
            if is_progress && !pill_ok {
                return Response::error(
                    ErrorCode::Unsupported,
                    "Floating progress pill is turned off in Settings > Chat",
                );
            }
            if !is_progress && parsed.scope == ModalScope::Window && !window_ok {
                return Response::error(
                    ErrorCode::Unsupported,
                    "Persistent window modals are turned off in Settings > Chat",
                );
            }
            let mut parsed = parsed;
            if !is_progress && parsed.scope == ModalScope::All && !global_ok {
                if !window_ok {
                    return Response::error(
                        ErrorCode::Unsupported,
                        "Agent modals are turned off in Settings > Chat",
                    );
                }
                parsed.scope = ModalScope::Window;
            }
            let display_level = if is_progress || parsed.scope == ModalScope::All {
                EscalationLevel::GlobalHud
            } else {
                EscalationLevel::WindowModal
            };
            let id = store.update(cx, |s, cx| {
                let id = s.upsert(UpsertInteraction {
                    source,
                    agent_label: "Agent".to_owned(),
                    workspace_label,
                    workspace_root,
                    entry_id: None,
                    source_revision: None,
                    directive_index: None,
                    parsed,
                    display_level,
                    manual_open: true,
                    now,
                    volume,
                });
                sync_hud_deferred(cx);
                cx.notify();
                id
            });
            // Notify open windows so window-scoped overlays & Edge Light repaint.
            for win in MainWindow::all(cx) {
                let _ = win.update(cx, |_, _, cx| cx.notify());
            }
            Response::Modal {
                reply: Reply::Shown { id },
            }
        }
    }
}

fn into_parsed(op: ModalOp) -> Result<ParsedInteraction, &'static str> {
    let check_title = |title: &str| {
        if title.trim().is_empty() || title.len() > MAX_TITLE {
            Err("Modal title must be non-empty and at most 200 bytes")
        } else {
            Ok(title.trim().to_owned())
        }
    };
    match op {
        ModalOp::Show {
            scope,
            title,
            body,
            preset,
            choices,
            quick_reply,
            cue,
        } => {
            let directive = if scope == ModalScope::All {
                BlockDirective::Urgent
            } else {
                BlockDirective::Modal
            };
            Ok(ParsedInteraction {
                directive,
                scope,
                title: check_title(&title)?,
                body: body.chars().take(MAX_BODY).collect(),
                cue: cue.or_else(|| directive.default_cue()),
                auto_clear: true,
                payload: Payload::Modal {
                    preset,
                    choices,
                    quick_reply,
                },
            })
        }
        ModalOp::Countdown {
            scope,
            title,
            body,
            seconds,
        } => Ok(ParsedInteraction {
            directive: BlockDirective::Countdown,
            scope,
            title: check_title(&title)?,
            body: body.chars().take(MAX_BODY).collect(),
            cue: BlockDirective::Countdown.default_cue(),
            auto_clear: true,
            payload: Payload::Countdown {
                seconds: seconds.clamp(1, 300),
            },
        }),
        ModalOp::Otp { title, body } => Ok(ParsedInteraction {
            directive: BlockDirective::Otp,
            scope: ModalScope::All,
            title: check_title(&title)?,
            body: body.chars().take(MAX_BODY).collect(),
            cue: BlockDirective::Otp.default_cue(),
            auto_clear: true,
            payload: Payload::Otp,
        }),
        ModalOp::Oauth {
            title,
            body,
            url,
            code,
        } => Ok(ParsedInteraction {
            directive: BlockDirective::Oauth,
            scope: ModalScope::All,
            title: check_title(&title)?,
            body: body.chars().take(MAX_BODY).collect(),
            cue: BlockDirective::Oauth.default_cue(),
            auto_clear: true,
            payload: Payload::Oauth { url, code },
        }),
        ModalOp::Secret {
            title,
            key,
            dest,
            reason,
            save_keychain,
        } => {
            crate::agent_interactions::vault::validate_key(&key)
                .map_err(|_| "Secret key must match [A-Za-z_][A-Za-z0-9_]*")?;
            crate::agent_interactions::vault::parse_destination(&dest)
                .map_err(|_| "Secret dest must be env-file:<relative> or keychain:<name>")?;
            Ok(ParsedInteraction {
                directive: BlockDirective::Secret,
                scope: ModalScope::Window,
                title: check_title(&title)?,
                body: reason.chars().take(MAX_BODY).collect(),
                cue: BlockDirective::Secret.default_cue(),
                auto_clear: true,
                payload: Payload::Secret {
                    key,
                    dest,
                    save_keychain,
                },
            })
        }
        ModalOp::Progress {
            title,
            percent,
            step,
            done,
        } => Ok(ParsedInteraction {
            directive: BlockDirective::Progress,
            scope: ModalScope::All,
            title: check_title(&title)?,
            body: String::new(),
            cue: done.then_some(sheprd_control_protocol::protocol::modal::CueKind::Done),
            auto_clear: false,
            payload: Payload::Progress {
                percent: percent.map(|p| p.min(100)),
                step,
                done,
            },
        }),
        ModalOp::Runbook { title, steps } => Ok(ParsedInteraction {
            directive: BlockDirective::Runbook,
            scope: ModalScope::Window,
            title: check_title(&title)?,
            body: String::new(),
            cue: None,
            auto_clear: false,
            payload: Payload::Runbook { steps },
        }),
        ModalOp::Drop {
            scope,
            title,
            accept,
        } => Ok(ParsedInteraction {
            directive: BlockDirective::Drop,
            scope,
            title: check_title(&title)?,
            body: String::new(),
            cue: None,
            auto_clear: true,
            payload: Payload::Drop { accept },
        }),
        ModalOp::Diff { title, file, diff } => Ok(ParsedInteraction {
            directive: BlockDirective::Diff,
            scope: ModalScope::Window,
            title: check_title(&title)?,
            body: String::new(),
            cue: BlockDirective::Diff.default_cue(),
            auto_clear: true,
            payload: Payload::Diff {
                file,
                diff: diff.chars().take(MAX_BODY).collect(),
            },
        }),
        ModalOp::Spotlight {
            title,
            target,
            body,
            step,
        } => Ok(ParsedInteraction {
            directive: BlockDirective::Spotlight,
            scope: ModalScope::Window,
            title: check_title(&title)?,
            body: body.chars().take(MAX_BODY).collect(),
            cue: None,
            auto_clear: true,
            payload: Payload::Spotlight { target, step },
        }),
        ModalOp::List | ModalOp::Clear { .. } | ModalOp::Cue { .. } => {
            Err("Unexpected modal operation")
        }
    }
}
