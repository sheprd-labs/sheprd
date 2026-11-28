//! The plugin section every menu draws below its own rows: a rule, then each
//! plugin's name over its actions. Rows look and behave as the menu's own:
//! hover and the arrow keys select the same row. A long title ends with "…"
//! instead of widening the menu, and its tooltip holds the whole title and
//! the plugin's description of the action.

use super::Row;
use crate::{SheprdWindow, direction::Logical};
use gpui::{prelude::*, *};

/// The widest a plugin title or name may make a menu, at a 12px UI font.
const TITLE_WIDTH: f32 = 320.;

/// Which menu's rows the section matches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Layout {
    /// A context menu's flat rows.
    Menu,
    /// The workspace popover's rows, whose labels line up after an icon slot.
    Popover {
        inset: f32,
        icon_slot: f32,
        gap: f32,
    },
}

/// How many section children come before row `index`'s own and include it:
/// each plugin's heading is a child of its own, before its first row.
pub(crate) fn child_index(rows: &[Row], index: usize) -> usize {
    index
        + rows
            .iter()
            .take(index + 1)
            .filter(|row| row.heading.is_some())
            .count()
}

impl SheprdWindow {
    /// The section's children, in order, for rows `rows` with `selected` the
    /// selected row's index among them. `hover` and `click` make each row's
    /// listeners from its index.
    pub(crate) fn render_plugin_rows<H, C>(
        &self,
        rows: &[Row],
        selected: Option<usize>,
        layout: Layout,
        hover: impl Fn(usize) -> H,
        click: impl Fn(usize) -> C,
    ) -> Vec<AnyElement>
    where
        H: Fn(&bool, &mut Window, &mut App) + 'static,
        C: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    {
        let theme = &self.theme;
        let width = px(TITLE_WIDTH * (self.config.ui.size / 12.));
        let lead = match layout {
            Layout::Menu => 8.,
            Layout::Popover {
                inset,
                icon_slot,
                gap,
            } => inset + icon_slot + gap,
        };
        let mut children = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            if let Some(name) = &row.heading {
                let plugin = row.plugin_id.clone();
                children.push(
                    div()
                        .flex_none()
                        .debug_selector(move || format!("plugin-heading-{plugin}"))
                        .when(index == 0, |heading| {
                            heading
                                .mt(px(4.))
                                .border_t_1()
                                .border_color(rgb(theme.active))
                        })
                        .pt(px(6.))
                        .pb(px(2.))
                        .ps(px(lead))
                        .pe(px(8.))
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(div().max_w(width).truncate().child(name.clone()))
                        .into_any_element(),
                );
            }
            let key = format!("{}/{}", row.plugin_id, row.action_id);
            let (label_key, hint) = (key.clone(), tooltip_text(row));
            let (foreground, surface) = (theme.foreground, theme.surface);
            let is_selected = selected == Some(index);
            children.push(
                div()
                    .id(("plugin-action", index))
                    .flex_none()
                    .debug_selector(move || format!("plugin-action-{key}"))
                    .role(Role::MenuItem)
                    .aria_label(row.title.clone())
                    .when(is_selected, |row| row.aria_active_descendant())
                    .min_h(px(self.config.ui.line_height() + 12.))
                    .row()
                    .items_center()
                    .cursor_pointer()
                    .map(|div| match layout {
                        Layout::Menu => div.px(px(8.)).hover(|row| row.bg(rgb(theme.active))),
                        Layout::Popover { inset, .. } => div
                            .mx(px(2.))
                            .ps(px(lead))
                            .pe(px(inset))
                            .rounded(px(crate::config::corners::CONTROL)),
                    })
                    .when(is_selected, |row| row.bg(rgb(theme.active)))
                    .child(
                        div()
                            .debug_selector(move || format!("plugin-action-label-{label_key}"))
                            .min_w_0()
                            .max_w(width)
                            .truncate()
                            .child(row.title.clone()),
                    )
                    .tooltip(move |_, cx| {
                        cx.new(|_| crate::usage::Hint {
                            text: hint.clone(),
                            foreground,
                            surface,
                        })
                        .into()
                    })
                    .on_hover(hover(index))
                    .on_click(click(index))
                    .into_any_element(),
            );
        }
        children
    }
}

/// The whole title, and the plugin's description of the action under it.
fn tooltip_text(row: &Row) -> SharedString {
    match &row.description {
        Some(description) => format!("{}\n{description}", row.title).into(), // i18n: data
        None => row.title.clone().into(),
    }
}
