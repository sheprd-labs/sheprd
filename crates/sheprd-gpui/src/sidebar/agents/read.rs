//! Unread agents in the agents panel: a dot at the row's end while the
//! agent has a reply no one read, on any device. Each host's daemon keeps
//! what was read (`crate::agent_read`); an agent focused here a moment is
//! read.

use super::listed::Listed;
use crate::direction::Logical;
use crate::i18n::fl;
use crate::{
    SheprdWindow,
    agent_read::{Agent, AgentRead},
};
use gpui::{prelude::*, *};

impl SheprdWindow {
    /// Follows the unread agents as their daemons answer.
    pub(crate) fn observe_agent_read(cx: &mut Context<Self>) -> Subscription {
        cx.observe_global::<AgentRead>(|this, cx| {
            this.agent_read = AgentRead::current(cx);
            cx.notify();
        })
    }

    /// `agent`, of the host at `index`, as read marks know it.
    fn read_agent(&self, index: usize, agent: &Listed<'_>) -> Option<Agent> {
        Some(Agent {
            endpoint: self.endpoints.get(index)?.id.clone(),
            key: agent.favorite_key()?,
        })
    }

    /// Tells which agent this window shows focused, while it is in front
    /// and the selected host has any unread: only then can focus read one.
    pub(crate) fn report_agent_focus(&self, window: &Window, cx: &mut Context<Self>) {
        let id = window.window_handle().window_id();
        let unread_here = self
            .endpoints
            .get(self.selected_endpoint)
            .is_some_and(|endpoint| self.agent_read.any(&endpoint.id));
        let focused = (self.active && unread_here)
            .then(|| {
                self.panel_agents()
                    .into_iter()
                    .find(|(index, agent)| *index == self.selected_endpoint && agent.focused())
                    .and_then(|(index, agent)| self.read_agent(index, &agent))
            })
            .flatten();
        AgentRead::focus(cx, id, focused);
    }

    /// The dot at an unread agent's row end, which marks it read; `None`
    /// for an agent with nothing unread. It sits under the star's corner.
    pub(in crate::sidebar) fn unread_dot(
        &self,
        index: usize,
        agent: &Listed<'_>,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let target = self.read_agent(index, agent)?;
        if !self.agent_read.contains(&target.endpoint, &target.key) {
            return None;
        }
        let label = fl!("agent-read-mark-read", agent = agent.name());
        let selector = format!("agent-unread-{}-{}", target.endpoint, target.key);
        Some(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .role(Role::Button)
                .aria_label(label)
                .absolute()
                .bottom(px(2.))
                .inset_e(px(2.))
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .bg(rgb(self.theme.palette[4])),
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    // The row under the dot opens its agent on click.
                    cx.stop_propagation();
                    AgentRead::set(cx, &target, true);
                })),
        )
    }
}
