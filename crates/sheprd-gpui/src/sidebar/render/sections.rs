//! The two lists shared by the classic sidebar and the stock Moonframe mods.
use super::*;

impl SheprdWindow {
    pub(crate) fn sidebar_list_root(&self) -> Div {
        let font = &self.config.sidebar;
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .text_color(rgb(self.theme.foreground))
            .bg(self.fill().panel(self.theme.sidebar_background()))
    }

    /// Reveals the selected row once, after its viewport has been measured.
    pub(super) fn reveal_sidebar_row(
        &self,
        list: usize,
        row: Option<usize>,
        cover: Pixels,
        headed: bool,
        window: &mut Window,
    ) {
        let Some(row) = row else { return };
        if self.sidebar_revealed[list].get() != Some(row)
            && self.sidebar_scroll[list].bounds().size.height > px(0.)
        {
            let scroll = &self.sidebar_scroll[list];
            let visible = scroll.bounds_for_item(row).is_some_and(|bounds| {
                let offset = scroll.offset().y;
                bounds.bottom() + offset > scroll.bounds().top() + cover
                    && bounds.top() + offset < scroll.bounds().bottom()
            });
            // Even a partially visible worktree is already seen. GPUI's
            // reveal also moves clipped rows, so only request it off-screen.
            if !visible {
                scroll.scroll_to_item(row);
                // The scroll lands in prepaint, after this render pinned a
                // header for the old offset, and puts a row from above at
                // the top edge, under the pinned header. Another frame pins
                // for the new offset and moves the row out from under it.
                if list == 0 && headed {
                    window.request_animation_frame();
                    self.sidebar_pin_reveal.set(Some(row));
                }
            }
            self.sidebar_revealed[list].set(Some(row));
        }
    }

    pub(crate) fn render_agent_section(
        &self,
        indicators: Indicators,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let mut look = layout::for_config(&self.config, self.fill());
        let content_x = look.content_x();
        if self.endpoints.len() > 1 && self.device_tags.any() {
            look.group_indent = scaled(font, HOST_ARROW_WIDTH) + look.density.host_gap();
        }
        let filtered = self.agent_group_filter().is_some()
            || self
                .displayed_devices()
                .into_iter()
                .any(|(index, endpoint)| {
                    let live = if index == self.selected_endpoint {
                        &self.live
                    } else {
                        &endpoint.live
                    };
                    endpoint.enabled
                        && self.device_visible(&endpoint.id)
                        && !self.device_tag_folded(&endpoint.id)
                        && live
                            .snapshot
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.agent_view_label.is_some())
                        && !self.sessions(index).is_empty()
                });
        let agents = div()
            .id("agents-scroll")
            .debug_selector(|| "agents-scroll".into())
            .role(Role::List)
            .aria_label(fl!("sidebar-agents-label"))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        sticky::settle(&self.sidebar_scroll[1]);
        let (mut agents, agent_count, focused) = self.append_agent_rows(
            agents.track_scroll(&self.sidebar_scroll[1]),
            indicators,
            look,
            width,
            cx,
        );
        self.reveal_sidebar_row(1, focused, px(0.), false, window);
        if agent_count == 0 {
            agents = agents.child(
                div()
                    .px(px(content_x))
                    .text_color(rgb(theme.muted))
                    .truncate()
                    .child(label_text(&if filtered {
                        fl!("sidebar-no-matching-agents")
                    } else {
                        fl!("sidebar-no-agents")
                    })),
            );
        }
        div()
            .debug_selector(|| "agents-section".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            // A plugin view on any listed host, or a picked
            // group, filters the rows, so the header stays to
            // say so.
            .when(self.config.show_agents_header || filtered, |section| {
                section.child(
                    header("agents", &fl!("sidebar-header-agents"), font, theme, look)
                        .justify_between()
                        .child(
                            div()
                                .row()
                                .min_w_0()
                                .gap(px(look.density.gap()))
                                .children(agents_group(self, cx))
                                .child(agents_sort(self, cx)),
                        ),
                )
            })
            .child(agents)
    }
}
