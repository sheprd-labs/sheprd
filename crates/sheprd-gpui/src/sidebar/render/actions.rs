use super::*;

impl SheprdWindow {
    pub(crate) fn sidebar_actions(&self, cx: &mut Context<Self>) -> Div {
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let look = layout::for_config(&self.config, self.fill());
        div()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .flex_none()
            .h(px(line_height(font) + 2. * look.density.footer_padding()))
            .px(px(look.content_x()))
            .row()
            .items_center()
            // Menu hugs the sidebar's edge, as in the terminal client.
            .justify_between()
            .text_color(rgb(theme.muted))
            .gap(px(20.))
            .child(
                div()
                    .id("new-workspace")
                    .map(|button| self.sidebar_button(button, fl!("sidebar-new-workspace")))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme.foreground)))
                    .child(fl!("sidebar-new"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.command(Command::Workspace, window, cx)
                    })),
            )
            .child(
                div()
                    .id("sidebar-menu")
                    .debug_selector(|| "sidebar-menu".into())
                    .map(|button| self.sidebar_button(button, fl!("sidebar-menu-label")))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme.foreground)))
                    .child(label_text(&fl!("sidebar-menu")))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        this.menu.anchor = event.position();
                        this.open_menu(window, cx);
                    })),
            )
    }
}
