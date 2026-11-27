//! Indicators: the set agent status is drawn with, as cards to click, and the
//! edge rail. Each card's preview is the sidebar's own marks, so a mark that
//! moves there moves here too.
// i18n: migrated
use super::{SettingsWindow, on_value, ui};
use crate::direction::Logical;
use crate::{
    config::{StatusIcons, corners, preferences::Preference},
    i18n::{self, fl},
    settings_window::{
        controls::indicators::set_messages, inputs::NumberField, sidebar::HexTarget,
    },
    sidebar::{Indicators, status_indicator},
};
use gpui::{prelude::*, *};
use sheprd_client::protocol::AgentStatus;

/// The statuses each card previews, most urgent first, with the messages
/// naming them.
const PREVIEW: [(AgentStatus, &str); 5] = [
    (AgentStatus::Working, "settings-indicators-status-working"),
    (AgentStatus::Blocked, "settings-indicators-status-blocked"),
    (AgentStatus::Done, "settings-indicators-status-done"),
    (AgentStatus::Idle, "settings-indicators-status-idle"),
    (AgentStatus::Unknown, "settings-indicators-status-unknown"),
];
const CARD_WIDTH: f32 = 212.;

impl SettingsWindow {
    pub(in crate::settings_window) fn render_indicators(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let rail = self.config.edge_rail;
        let light = matches!(
            cx.window_appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let indicators = Indicators::new(
            self.shared.as_ref(),
            self.config.status_icons,
            rail,
            light,
            theme,
        );
        div()
            .flex()
            .flex_col()
            .child(
                ui::row(
                    theme,
                    "indicators-edge-rail",
                    fl!("settings-indicators-edge-rail"),
                )
                .tag(ui::Source::Sheprd)
                .description(fl!("settings-indicators-edge-rail-description"))
                .switch(self.preference_toggle(
                    "indicators-edge-rail-toggle",
                    rail,
                    Preference::EdgeRail(!rail),
                    cx,
                )),
            )
            .child(self.edge_light_controls(cx))
            .child(ui::section(
                theme,
                &fl!("settings-indicators-sets"),
                Some(ui::Source::Sheprd),
            ))
            .child(
                div()
                    .relative()
                    .debug_selector(|| "indicators-sets".into())
                    // Search scrolls to the cards by these bounds.
                    .child(crate::settings_window::search::track_bounds(
                        "indicators-sets".into(),
                    ))
                    .row()
                    .flex_wrap()
                    .gap(px(10.))
                    .pt(px(4.))
                    .children(
                        StatusIcons::ALL
                            .iter()
                            .map(|&set| self.set_card(set, indicators.with_icons(set), cx)),
                    ),
            )
    }

    fn edge_light_controls(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let light = self.config.edge_light;
        let ready = !self.busy() && self.closing.is_none();
        let mut color = ui::row(
            theme,
            "indicators-edge-light-color",
            fl!("settings-indicators-edge-light-color"),
        )
        .description(fl!("settings-indicators-edge-light-color-description"));
        if let Some(rgb) = light.color {
            color = color.dot(rgb);
        }
        let presets = [
            (0x63baff, "#63BAFF"),
            (0xae81ff, "#AE81FF"),
            (0xff72a8, "#FF72A8"),
            (0xffad53, "#FFAD53"),
            (0x67d9a6, "#67D9A6"),
        ];
        color = color.control(div().row().items_center().gap(px(4.)).children(presets.map(
            |(tint, label)| {
                ui::swatch(theme, tint, light.color == Some(tint))
                    .id(("edge-light-swatch", tint as usize))
                    .role(Role::Button)
                    .aria_label(label)
                    .debug_selector(move || format!("edge-light-swatch-{tint:06x}"))
                    .when(ready, |swatch| {
                        swatch
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.save_preference(Preference::EdgeLightColor(Some(tint)), cx);
                            }))
                    })
            },
        )));
        if let Some(edit) = self
            .hex_edit
            .as_ref()
            .filter(|edit| matches!(edit.target, HexTarget::EdgeLight))
        {
            color = color.control(
                div()
                    .w(px(104.))
                    .rounded(px(corners::SMALL))
                    .border_1()
                    .border_color(rgb(if edit.invalid {
                        theme.ink(0xd45b5b)
                    } else {
                        theme.active
                    }))
                    .child(edit.input.clone()),
            );
        } else {
            let reset = ready && light.color.is_some();
            let label = light
                .color
                .map(|rgb| format!("#{rgb:06X}"))
                .unwrap_or_else(|| fl!("settings-indicators-edge-light-custom"));
            color = color
                .control(
                    ui::button(
                        theme,
                        "edge-light-auto",
                        fl!("settings-indicators-edge-light-auto"),
                        reset,
                    )
                    .when(reset, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| {
                            this.save_preference(Preference::EdgeLightColor(None), cx);
                        }))
                    }),
                )
                .control(
                    ui::button(theme, "edge-light-hex", label, ready).when(ready, |button| {
                        button.on_click(cx.listener(move |this, _, window, cx| {
                            this.begin_color_edit(HexTarget::EdgeLight, light.color, window, cx);
                        }))
                    }),
                );
        }
        let mut controls = div()
            .flex()
            .flex_col()
            .child(ui::section(
                theme,
                &fl!("settings-indicators-edge-light"),
                Some(ui::Source::Sheprd),
            ))
            .child(color);
        for (field, id, title, description, saved, unit) in [
            (
                NumberField::EdgeLightWidth,
                "indicators-edge-light-width",
                fl!("settings-indicators-edge-light-width"),
                fl!("settings-indicators-edge-light-width-description"),
                light.width,
                ui::Unit::Pixels,
            ),
            (
                NumberField::EdgeLightTrail,
                "indicators-edge-light-trail",
                fl!("settings-indicators-edge-light-trail"),
                fl!("settings-indicators-edge-light-trail-description"),
                light.trail,
                ui::Unit::Percent,
            ),
        ] {
            let value = self.number_value(field, f32::from(saved));
            let view = cx.entity().downgrade();
            controls = controls.child(ui::row(theme, id, title).description(description).control(
                ui::number(
                    theme,
                    ui::Number {
                        id: field.selector(),
                        value,
                        range: field.range(),
                        unit,
                        look: ui::Look::Slider,
                        enabled: ready,
                    },
                    self.number_edit_for(field),
                    on_value(cx, move |this, change, _, cx| {
                        this.commit_number(field, change, cx)
                    }),
                    move |window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.begin_number_edit(field, value, window, cx)
                        });
                    },
                ),
            ));
        }
        controls
    }

    /// One set as a card: its name and a radio mark, the five statuses drawn
    /// with it, and a line on what sets it apart. Clicking picks it.
    fn set_card(
        &self,
        set: StatusIcons,
        indicators: Indicators,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let on = self.config.status_icons == set;
        let font = &self.config.sidebar;
        let (name, description) = set_messages(set);
        div()
            .id(("indicator-set", set as usize))
            .debug_selector(move || format!("indicator-set-{}", set.as_str()))
            .w(px(CARD_WIDTH))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(if on { theme.primary() } else { theme.active }))
            .bg(rgb(theme.surface))
            .when(!on, |card| {
                card.cursor_pointer()
                    .hover(|card| card.border_color(rgb(theme.muted)))
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_status_icons(set, cx)))
            })
            .child(
                div()
                    .row()
                    .items_center()
                    .justify_between()
                    .text_color(rgb(theme.foreground))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(i18n::message(name, &[]))
                    .child(radio(theme, on)),
            )
            .child(
                div()
                    .row()
                    .justify_between()
                    .px(px(4.))
                    .py(px(6.))
                    .rounded(px(corners::SMALL))
                    .bg(rgb(theme.background))
                    .children(PREVIEW.map(|(status, message)| {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(3.))
                            .child(status_indicator(status, font, indicators))
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(rgb(theme.muted))
                                    .child(i18n::message(message, &[])),
                            )
                    })),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(theme.subtext()))
                    .child(i18n::message(description, &[])),
            )
    }
}

fn radio(theme: &crate::config::Theme, on: bool) -> Div {
    div()
        .size(px(14.))
        .row()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(rgb(if on { theme.primary() } else { theme.muted }))
        .when(on, |radio| {
            radio.child(div().size(px(6.)).rounded_full().bg(rgb(theme.primary())))
        })
}

#[cfg(test)]
mod tests;
