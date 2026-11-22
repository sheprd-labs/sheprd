//! The header over the turns a continued conversation brought, which opens
//! and folds them. It sits above the brought conversation's last prompt,
//! the first row the fold leaves in view.

use super::{
    super::earlier::Earlier,
    disclosure::chevron,
    layout::{Row, Side, layout_for},
};
use crate::{SheprdWindow, i18n::fl};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{Entry, EntryKind, TurnEnd, TurnSummary};

impl SheprdWindow {
    /// Row `index` of the chat in `pane_id`: nothing while a fold hides it,
    /// and under the fold's header when it is the first row in view.
    pub(super) fn render_chat_entry(
        &self,
        pane_id: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fold = self.chats.pane(pane_id).and_then(|chat| chat.earlier());
        let Some(fold) = fold else {
            return self.render_chat_entry_shown(pane_id, index, cx);
        };
        if fold.hides(index) {
            return div().into_any_element();
        }
        let row = self.render_chat_entry_shown(pane_id, index, cx);
        if index != fold.start {
            return row;
        }
        // Full width, as a row alone is: a prompt keeps its side.
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.render_chat_earlier(pane_id, fold, cx))
            .child(row)
            .into_any_element()
    }

    fn render_chat_earlier(
        &self,
        pane_id: &str,
        fold: Earlier,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let pane = pane_id.to_owned();
        let id = fold.divider;
        let label = if fold.open {
            fl!("chat-earlier-hide", count = fold.turns)
        } else {
            fl!("chat-earlier-show", count = fold.turns)
        };
        let header = div()
            .id(("chat-earlier", id.0))
            .debug_selector(|| "chat-earlier".into())
            .role(Role::Button)
            .aria_label(label.clone())
            .aria_expanded(fold.open)
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .hover(|header| header.text_color(rgb(theme.foreground)))
            .child(chevron(fold.open, theme))
            .child(label)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if let Some(chat) = this.chats.pane_mut(&pane)
                    && let Some(fold) = chat.earlier()
                {
                    chat.toggle_earlier(fold);
                }
                cx.notify();
            }));
        let layout = layout_for(self.config.chat.layout);
        let look = self.chat_look();
        // The header is no entry of the transcript; it borrows the quiet
        // rule a layout draws between turns, in that layout's own row.
        let rule = Entry {
            id,
            kind: EntryKind::Turn(TurnSummary {
                duration_ms: 0,
                ended_ms: None,
                end: TurnEnd::Done,
                tokens: None,
            }),
        };
        let agent = self
            .chats
            .pane(pane_id)
            .map(|chat| chat.session.agent().to_owned())
            .unwrap_or_default();
        layout.row(
            Row {
                entry: &rule,
                previous: None,
                next: Some(Side::User),
                agent: &agent,
                live: false,
                current: false,
                expanded: fold.open,
                tool: None,
                content: layout.turn(header.into_any_element(), &look),
            },
            &look,
        )
    }
}
