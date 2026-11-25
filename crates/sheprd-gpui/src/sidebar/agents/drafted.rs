//! A pencil at the end of an agent's row while its chat holds a draft not
//! sent (`crate::chat::drafts`), as the phone marks its own.

use super::listed::Listed;
use crate::chat::drafts::Place;
use crate::direction::Logical;
use crate::i18n::fl;
use crate::{SheprdWindow, browser::scope, core_terminals::Host};
use gpui::{prelude::*, *};
use sheprd_backend::TerminalId;

/// The room one mark at a row's bottom end takes: its slot and the inset
/// before it.
const MARK_SLOT: f32 = 16.;

/// How much narrower a drafted agent's text lays out, so it ends before the
/// pencil and, when both show, the unread dot after it.
pub(in crate::sidebar) fn draft_reserve(unread: bool) -> f32 {
    MARK_SLOT * if unread { 2. } else { 1. }
}

impl SheprdWindow {
    /// Where the chat of `agent`, on the host at `index`, keeps its draft:
    /// the place `chat::drafts` names for the same pane.
    pub(crate) fn listed_draft_place(&self, index: usize, agent: &Listed<'_>) -> Option<Place> {
        let endpoint = self.endpoints.get(index)?;
        Some(match agent {
            Listed::Herdr1 { agent, .. } => Place::Herdr {
                endpoint: endpoint.id.clone(),
                pane: agent.pane_id.clone(),
            },
            Listed::Core(core) => Place::Core {
                key: crate::chat::core_pane::key_on(
                    &Host::of(&scope(endpoint)),
                    &TerminalId::new(core.agent.pane.as_str()).ok()?,
                ),
            },
        })
    }

    /// Whether `agent`'s chat holds a draft not sent.
    pub(crate) fn drafted(&self, index: usize, agent: &Listed<'_>) -> bool {
        self.listed_draft_place(index, agent)
            .is_some_and(|place| self.chat_drafts.contains(&place))
    }

    /// The pencil at a drafted agent's row end, at the bottom like the
    /// unread dot and before it when both show; `None` without a draft.
    pub(in crate::sidebar) fn draft_pencil(
        &self,
        index: usize,
        agent: &Listed<'_>,
        unread: bool,
    ) -> Option<Stateful<Div>> {
        if !self.drafted(index, agent) {
            return None;
        }
        let selector = format!(
            "agent-drafted-{}-{}",
            self.endpoints.get(index)?.id,
            agent.id()
        );
        Some(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .role(Role::Image)
                .aria_label(fl!("chat-drafts-mark"))
                .absolute()
                .bottom(px(2.))
                .inset_e(px(if unread { MARK_SLOT + 2. } else { 2. }))
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .child(crate::icons::icon(
                    "icons/pencil.svg",
                    10.,
                    self.theme.primary(),
                )),
        )
    }
}
