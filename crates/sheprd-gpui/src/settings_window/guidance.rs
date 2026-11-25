//! Agent navigation uses the same pages, row catalog, and scrolling as search.
use crate::{
    SheprdWindow, agent_guidance,
    control::protocol::{
        ErrorCode, Response,
        app::{Reply, SettingsTarget, UiOp},
    },
    settings_window::{
        Section, SettingsWindow, SettingsWindowHandle, catalog, open_now, search::RowBounds,
    },
};
use gpui::{prelude::*, *};

pub(crate) fn apply(source: WeakEntity<SheprdWindow>, op: UiOp, cx: &mut App) -> Response {
    let Some(owner) = source.upgrade() else {
        // i18n: data
        return Response::error(ErrorCode::NoWindow, "The window closed");
    };
    let guide = owner.read(cx).agent_guidance.clone();
    let (page, text, target) = match op {
        UiOp::Guide {
            text: None,
            highlight: None,
        } => {
            guide.update(cx, |state, cx| state.clear(cx));
            // i18n: data
            return Response::done("Cleared agent guidance");
        }
        UiOp::Guide { text, highlight } => {
            if text
                .as_ref()
                .is_some_and(|text| !sheprd_remote_control::control::phone::valid_text(text))
            {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    // i18n: data
                    "Guidance must be nonblank and at most 500 characters",
                );
            }
            let target = match highlight {
                Some(id) => match catalog::CATALOG.iter().position(|row| row.id == id) {
                    Some(index) => Some(index),
                    None => {
                        return Response::error(
                            ErrorCode::NotFound,
                            // i18n: data
                            "Unknown Settings row; use `ui settings` to list targets",
                        );
                    }
                },
                None => None,
            };
            if target.is_none() {
                guide.update(cx, |state, cx| state.update(text, None, cx));
                // i18n: data
                return Response::done("Updated agent guidance");
            }
            (None, text, target)
        }
        UiOp::Settings { page } => {
            let section = match page {
                Some(page) => match Section::ALL
                    .iter()
                    .copied()
                    .find(|section| section.name() == page)
                {
                    Some(section) => Some(section),
                    None => {
                        return Response::error(
                            ErrorCode::NotFound,
                            // i18n: data
                            "Unknown Settings page; use `ui settings` to list targets",
                        );
                    }
                },
                None => None,
            };
            (section, None, None)
        }
        // i18n: data
        _ => return Response::error(ErrorCode::InvalidRequest, "Invalid guidance operation"),
    };
    open_now(source.clone(), cx);
    let Some(handle) = cx.default_global::<SettingsWindowHandle>().window else {
        // i18n: data
        return Response::error(ErrorCode::Failed, "Could not open Settings");
    };
    handle
        .update(cx, |view, window, cx| {
            if view.source.entity_id() != source.entity_id() {
                // i18n: data
                return Response::error(ErrorCode::Busy, "Settings is busy in another window");
            }
            if let Some(index) = target {
                let entry = &catalog::CATALOG[index];
                if !view.available(entry.availability) {
                    return Response::error(
                        ErrorCode::Unsupported,
                        // i18n: data
                        "This Settings row is not available now",
                    );
                }
                view.open_result(index, window, cx);
                let text = text.or_else(|| Some(entry.title()));
                guide.update(cx, |state, cx| state.update(text, Some(entry.id), cx));
                // i18n: data
                Response::done(format!("Highlighted {}", entry.id))
            } else if let Some(page) = page {
                view.select_section(page, window, cx);
                // i18n: data
                Response::done(format!("Opened Settings > {}", page.name()))
            } else {
                Response::App {
                    reply: Reply::SettingsTargets {
                        pages: Section::ALL
                            .iter()
                            .map(|page| page.name().to_owned())
                            .collect(),
                        rows: catalog::CATALOG
                            .iter()
                            .filter(|entry| view.available(entry.availability))
                            .map(|entry| SettingsTarget {
                                id: entry.id.to_owned(),
                                page: entry.section.name().to_owned(),
                                title: entry.title(),
                            })
                            .collect(),
                    },
                }
            }
        })
        // i18n: data
        .unwrap_or_else(|_| Response::error(ErrorCode::NoWindow, "Settings closed"))
}

impl SettingsWindow {
    pub(super) fn guidance_highlight(
        &self,
        guide: Option<&Entity<agent_guidance::State>>,
        cx: &App,
    ) -> Option<impl IntoElement> {
        let target = guide?.read(cx).target?;
        if self.search_results.is_some()
            || !catalog::CATALOG.iter().any(|entry| {
                entry.id == target
                    && entry.section == self.section
                    && self.available(entry.availability)
            })
        {
            return None;
        }
        let scroll = self.body_scroll.clone();
        let color = rgba(if self.drafted_is_light(cx) {
            0xff950029
        } else {
            0xff9f0a3d
        });
        Some(
            canvas(
                |_, _, _| (),
                move |_, (), window, cx| {
                    let Some(row) = cx
                        .try_global::<RowBounds>()
                        .and_then(|rows| rows.0.get(target))
                        .copied()
                    else {
                        return;
                    };
                    window.with_content_mask(
                        Some(ContentMask {
                            bounds: scroll.bounds(),
                        }),
                        |window| {
                            window.paint_quad(fill(row, color).corner_radii(px(8.)));
                        },
                    );
                },
            )
            .absolute()
            .size_full(),
        )
    }
}
