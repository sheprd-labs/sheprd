//! The tab menu's rows: its own actions, then the plugin actions for a tab.
//! The rows are the menu panel's own children, which its scroll handle
//! addresses by index, so a row the keyboard selects below the window's edge
//! scrolls into view.

use super::TabMenu;
use crate::{
    SheprdWindow,
    plugin_actions::{self, Layout, Scope, Target},
};
use gpui::{prelude::*, *};

impl TabMenu {
    /// The panel child that draws row `index`: the menu's own rows come
    /// first, one child each, then the plugin section with its headings.
    pub(super) fn child_index(&self, index: usize) -> usize {
        let own = self.actions().len();
        match index.checked_sub(own) {
            Some(plugin) => own + plugin_actions::child_index(&self.plugins, plugin),
            None => index,
        }
    }
}

impl SheprdWindow {
    /// Takes the open tab menu's plugin rows from its host's last answer.
    pub(crate) fn sync_tab_plugin_rows(&mut self) {
        if self.menu.tab.is_none() {
            return;
        }
        let rows = self.plugin_rows(Scope::Tab);
        let Some(tab) = &mut self.menu.tab else {
            return;
        };
        if tab.plugins == rows {
            return;
        }
        // The row the keyboard selected may now be another action. The move
        // picker's selection counts its own rows, so it stays.
        if self.menu.page == Some(crate::menu::Page::Tab)
            && tab
                .selected
                .is_some_and(|index| index >= tab.actions().len())
        {
            tab.selected = None;
        }
        tab.plugins = rows;
    }

    /// Runs plugin row `index` on the menu's tab.
    pub(super) fn activate_tab_plugin(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = &self.menu.tab else { return };
        if tab.moving.is_some() {
            return;
        }
        let Some(row) = tab.plugins.get(index).cloned() else {
            return;
        };
        let tab_id = match self.validate_tab_target() {
            Ok(target) => target.tab.clone(),
            Err(error) => {
                self.tab_error(error, cx);
                return;
            }
        };
        if let Err(error) = self.run_plugin_action(&row, Target::Tab(tab_id), None, window, cx) {
            self.tab_error(error, cx);
        }
    }

    /// Fills the menu's `panel` with the menu's rows.
    pub(crate) fn render_tab_actions(
        &self,
        panel: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let Some(tab) = &self.menu.tab else {
            return panel;
        };
        if tab.moving.is_some() {
            return panel.child(self.render_tab_move_pending());
        }
        let theme = &self.theme;
        let mut panel = panel.track_scroll(&tab.actions_scroll).flex().flex_col();
        let actions = tab.actions();
        let own = actions.len();
        for (index, action) in actions.into_iter().enumerate() {
            panel = panel.child(
                div()
                    .id(("tab-menu-action", index))
                    .flex_none()
                    .debug_selector(move || format!("tab-menu-{index}"))
                    .min_h(px(self.config.ui.line_height() + 12.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .when(tab.selected == Some(index), |row| row.bg(rgb(theme.active)))
                    .hover(|row| row.bg(rgb(theme.active)))
                    .child(
                        div()
                            .debug_selector(move || format!("tab-menu-label-{index}"))
                            .min_w_0()
                            .truncate()
                            .child(action.label()),
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered && let Some(tab) = &mut this.menu.tab {
                            tab.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_tab_menu(action, window, cx)
                    })),
            );
        }
        let listeners: &Context<Self> = cx;
        panel = panel.children(self.render_plugin_rows(
            &tab.plugins,
            tab.selected.and_then(|index| index.checked_sub(own)),
            Layout::Menu,
            |index| {
                listeners.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered && let Some(tab) = &mut this.menu.tab {
                        tab.selected = Some(own + index);
                        cx.notify();
                    }
                })
            },
            |index| {
                listeners.listener(move |this, _: &ClickEvent, window, cx| {
                    this.activate_tab_plugin(index, window, cx)
                })
            },
        ));
        panel.when_some(tab.error.clone(), |panel, error| {
            // No width of its own, so the labels set the panel's width and
            // the message wraps inside it.
            panel.child(div().flex_none().w_0().min_w_full().p(px(8.)).child(error))
        })
    }
}
