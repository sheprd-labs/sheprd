//! The pane menu's rows: its own actions, then the plugin actions for a pane
//! (and for its selection, when text is highlighted in it). The rows are the
//! menu panel's own children, which its scroll handle addresses by index, so
//! a row the keyboard selects below the window's edge scrolls into view.

use super::PaneMenu;
use crate::direction::Logical;
use crate::{
    SheprdWindow,
    plugin_actions::{self, Layout, Scope, Target},
};
use gpui::{prelude::*, *};

impl PaneMenu {
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
    /// Takes the open pane menu's plugin rows from its host's last answer.
    pub(crate) fn sync_pane_plugin_rows(&mut self) {
        let Some(pane) = &self.menu.pane else { return };
        let rows = self.plugin_rows(Scope::Pane {
            selection: pane.selection.is_some(),
        });
        let Some(pane) = &mut self.menu.pane else {
            return;
        };
        if pane.plugins == rows {
            return;
        }
        // The row the keyboard selected may now be another action. The move
        // picker's selection counts its own rows, so it stays.
        if self.menu.page == Some(crate::menu::Page::Pane)
            && pane
                .selected
                .is_some_and(|index| index >= pane.actions().len())
        {
            pane.selected = None;
        }
        pane.plugins = rows;
    }

    /// Runs plugin row `index` on the menu's pane, with its selection when
    /// the action takes one.
    pub(super) fn activate_pane_plugin(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = &self.menu.pane else { return };
        if pane.moving.is_some() {
            return;
        }
        let Some(row) = pane.plugins.get(index).cloned() else {
            return;
        };
        let mut selection = pane.selection.clone();
        let pane_id = match self.validate_pane_target() {
            Ok(target) => target.pane.clone(),
            Err(error) => {
                self.pane_error(error, cx);
                return;
            }
        };
        // Output while the menu was open must not refuse the action.
        if let Some(selection) = &mut selection {
            self.fence_plugin_selection(selection);
        }
        if let Err(error) =
            self.run_plugin_action(&row, Target::Pane(pane_id), selection, window, cx)
        {
            self.pane_error(error, cx);
        }
    }

    /// Fills the menu's `panel` with the menu's rows.
    pub(crate) fn render_pane_actions(
        &self,
        panel: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let Some(pane) = &self.menu.pane else {
            return panel;
        };
        if pane.moving.is_some() {
            return panel.child(self.render_pane_move_pending());
        }
        let mut panel = panel.track_scroll(&pane.actions_scroll).flex().flex_col();
        let actions = pane.actions();
        let own = actions.len();
        for (index, action) in actions.into_iter().enumerate() {
            panel = panel.child(
                div()
                    .id(("pane-menu-action", index))
                    .flex_none()
                    .debug_selector(move || format!("pane-menu-{index}"))
                    .min_h(px(self.config.ui.line_height() + 12.))
                    .px(px(8.))
                    .row()
                    .items_center()
                    .cursor_pointer()
                    .when(pane.selected == Some(index), |row| {
                        row.bg(rgb(self.theme.active))
                    })
                    .hover(|row| row.bg(rgb(self.theme.active)))
                    .child(
                        // One line: the panel widens to fit it, and a
                        // window too narrow for that ends it with "…".
                        div()
                            .debug_selector(move || format!("pane-menu-label-{index}"))
                            .min_w_0()
                            .truncate()
                            .child(action.label(&pane.target)),
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered && let Some(pane) = &mut this.menu.pane {
                            pane.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_pane_menu(action, window, cx)
                    })),
            );
        }
        let listeners: &Context<Self> = cx;
        panel = panel.children(self.render_plugin_rows(
            &pane.plugins,
            pane.selected.and_then(|index| index.checked_sub(own)),
            Layout::Menu,
            |index| {
                listeners.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered && let Some(pane) = &mut this.menu.pane {
                        pane.selected = Some(own + index);
                        cx.notify();
                    }
                })
            },
            |index| {
                listeners.listener(move |this, _: &ClickEvent, window, cx| {
                    this.activate_pane_plugin(index, window, cx)
                })
            },
        ));
        panel.when_some(pane.error.clone(), |panel, error| {
            // No width of its own, so the labels set the panel's width and
            // the message wraps inside it.
            panel.child(div().flex_none().w_0().min_w_full().p(px(8.)).child(error))
        })
    }
}
