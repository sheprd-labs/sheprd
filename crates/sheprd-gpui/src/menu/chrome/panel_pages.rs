//! The pages that draw into the menu panel itself rather than as one child of
//! it: their rows are the panel's own children, so its scroll handle can
//! bring the row the keyboard selects into view.

use super::super::Page;
use crate::SheprdWindow;
use gpui::*;

/// Fills the menu panel with a page's rows.
type Render = fn(&SheprdWindow, Stateful<Div>, &mut Context<SheprdWindow>) -> Stateful<Div>;

impl Page {
    /// How this page fills the panel, if it draws into the panel itself.
    pub(super) fn panel_renderer(self) -> Option<Render> {
        Some(match self {
            // keep-sorted start
            Self::Group => SheprdWindow::render_group_menu,
            Self::MovePane => SheprdWindow::render_move_picker,
            Self::MoveTab => SheprdWindow::render_tab_move_picker,
            Self::Pane => SheprdWindow::render_pane_actions,
            Self::Tab => SheprdWindow::render_tab_actions,
            // keep-sorted end
            _ => return None,
        })
    }
}
