//! A tab that shows one of the app's own views rather than a page: code, the
//! devices overview, a herdr-core terminal, a machine, or a review.

use crate::{
    SheprdWindow,
    browser::{Location, Slot, Tab},
};
use gpui::{AnyElement, Context};

impl SheprdWindow {
    /// The view `tab` shows, by the kind of place it holds.
    pub(super) fn render_app_tab(
        &mut self,
        slot: Slot,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match tab.location {
            Some(Location::Code { .. }) => self.render_code_tab(slot, tab, cx),
            Some(Location::Devices) => self.render_devices_tab(slot, tab, cx),
            Some(Location::CoreTerminal { .. }) => self.render_core_terminal_tab(slot, tab, cx),
            Some(Location::Machine { .. }) => self.render_machine_tab(tab, cx),
            _ => self.render_review_tab(slot, tab, cx),
        }
    }
}
