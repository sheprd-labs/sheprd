//! The Pull requests list's State menu, which narrows it to one state, and
//! its Projects menu, which browses each Azure DevOps organization's
//! projects and lists a repository picked there without a checkout
//! (`remote`). Each dropdown hangs from its own control, as Sort does.

use super::{
    Menu, OrchestratorView,
    inbox::failure_text,
    remote::MAX_BROWSED,
    rows::{Filters, PrState},
};
use crate::{
    config::corners,
    i18n::fl,
    orchestrator::{Provider, RemoteScope, SourceKey, SyncState, azure},
};
use gpui::{prelude::*, *};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// The most repository rows the Projects menu draws; the rest are counted.
pub(super) const MAX_ROWS: usize = 400;
/// How far under its control a dropdown starts, the room it leaves between
/// itself and the window's edges, and the tallest it grows.
const MENU_DROP: f32 = 4.;
const MENU_GAP: f32 = 16.;
const MENU_MAX_HEIGHT: f32 = 420.;
/// The space between the rows of a menu where several may be picked, so
/// picked rows read as separate rather than as one block.
pub(super) const PICK_GAP: f32 = 4.;

/// Each dropdown's control and the room under it, to the window's bottom
/// edge, written as the control is drawn.
pub(crate) type MenuRoom = Rc<RefCell<HashMap<Menu, Placed>>>;

/// Where a dropdown's control was last drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    control: Bounds<Pixels>,
    below: Pixels,
}

/// A choice of the State menu, from the filters it sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateChoice {
    /// Open and draft, as the Open toggle lists.
    Open,
    All,
    Only(PrState),
}

impl StateChoice {
    pub(crate) const ALL: [Self; 5] = [
        Self::Open,
        Self::All,
        Self::Only(PrState::Draft),
        Self::Only(PrState::Merged),
        Self::Only(PrState::Closed),
    ];

    pub(crate) fn of(filters: Filters) -> Self {
        match (filters.state, filters.open_only) {
            (Some(state), _) => Self::Only(state),
            (None, true) => Self::Open,
            (None, false) => Self::All,
        }
    }

    pub(crate) fn apply(self, filters: &mut Filters) {
        filters.open_only = self == Self::Open;
        filters.state = match self {
            Self::Only(state) => Some(state),
            Self::Open | Self::All => None,
        };
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Open => fl!("orchestrator-browse-state-open"),
            Self::All => fl!("orchestrator-browse-state-all"),
            Self::Only(PrState::Draft) => fl!("orchestrator-browse-state-draft"),
            Self::Only(PrState::Merged) => fl!("orchestrator-browse-state-merged"),
            Self::Only(PrState::Closed) => fl!("orchestrator-browse-state-closed"),
        }
    }

    /// The id its menu row is found by in tests.
    fn id(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::All => "all",
            Self::Only(PrState::Draft) => "draft",
            Self::Only(PrState::Merged) => "merged",
            Self::Only(PrState::Closed) => "closed",
        }
    }
}

impl OrchestratorView {
    /// Opens `menu`, or shuts it when it is the one open.
    pub(super) fn toggle_menu(&mut self, menu: Menu) {
        self.menu = (self.menu != Some(menu)).then_some(menu);
        self.browse_rows = MAX_ROWS;
    }

    /// Draws another page of the Browse menu's repositories.
    pub(super) fn show_more_repositories(&mut self) {
        self.browse_rows += MAX_ROWS;
    }

    /// A dropdown's control: its text and a chevron.
    pub(super) fn menu_control(&self, id: &'static str, menu: Menu, text: String) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let open = self.menu == Some(menu);
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .text_size(look.small())
            .text_color(rgb(if open {
                theme.foreground
            } else {
                theme.subtext()
            }))
            .whitespace_nowrap()
            .relative()
            .child(self.measure_room(menu))
            .child(text)
            .child(look.icon("icons/chevron-down.svg", 10., theme.muted))
    }

    pub(super) fn render_state_control(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        self.menu_control(
            "orchestrator-state",
            Menu::State,
            StateChoice::of(self.filters).label(),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.toggle_menu(Menu::State);
            cx.notify();
        }))
        .when(self.menu == Some(Menu::State), |el| {
            el.child(self.render_state_menu(cx))
        })
    }

    /// The Projects control, once an organization is known.
    pub(super) fn render_projects_control(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if self.organizations().is_empty() {
            return None;
        }
        let look = &self.look;
        Some(
            self.menu_control(
                "orchestrator-projects",
                Menu::Projects,
                fl!("orchestrator-browse-projects"),
            )
            .child(look.icon("icons/azure-devops.svg", 11., look.theme.subtext()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_menu(Menu::Projects);
                cx.notify();
            }))
            .when(self.menu == Some(Menu::Projects), |el| {
                el.child(self.render_projects_menu(cx))
            }),
        )
    }

    /// Records the room under `menu`'s control, as a child filling it; the
    /// control must be `relative`.
    pub(super) fn measure_room(&self, menu: Menu) -> impl IntoElement {
        let rooms = self.menu_room.clone();
        canvas(
            move |control, window, _| {
                let placed = Placed {
                    control,
                    below: window.viewport_size().height - control.bottom(),
                };
                // Too late to draw this frame again, as when the search row
                // wraps: the menu takes the room it has from the next.
                if rooms.borrow_mut().insert(menu, placed) != Some(placed) {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// Hangs `frame` from the control holding it, right edges aligned, as
    /// the right edge stays put while the control's label changes width;
    /// moved as far as it must to stay inside the window. A press outside
    /// both shuts it; one on the control is left to the control's toggle.
    pub(super) fn drop_down(
        &self,
        menu: Menu,
        frame: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Div {
        let shut = cx.listener(move |this, event: &MouseDownEvent, _, cx| {
            let on_control = this
                .menu_room
                .borrow()
                .get(&menu)
                .is_some_and(|placed| placed.control.contains(&event.position));
            if this.menu != Some(menu) || on_control {
                return;
            }
            this.menu = None;
            cx.notify();
        });
        div().absolute().top_full().right_0().child(
            deferred(
                anchored()
                    .anchor(Anchor::TopRight)
                    .offset(point(px(0.), px(MENU_DROP)))
                    .snap_to_window_with_margin(px(MENU_GAP))
                    .child(div().on_mouse_down_out(shut).child(frame)),
            )
            .with_priority(1),
        )
    }

    /// A dropdown's frame, `width` wide, in the list's text rather than its
    /// control's smaller, muted one.
    pub(super) fn menu_frame(&self, width: f32) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        div()
            .w(px(width))
            .text_size(look.size())
            .text_color(rgb(theme.foreground))
            .p_1()
            .flex()
            .flex_col()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .shadow_lg()
    }

    /// A dropdown of `menu` whose `list` scrolls inside the room between
    /// its control and the window's bottom edge, at most 420 px, so a short
    /// window does not cut it off. `list` should carry `min_h_0` to shrink.
    pub(super) fn bounded_menu(
        &self,
        menu: Menu,
        width: f32,
        selector: &'static str,
        list: impl IntoElement,
    ) -> Div {
        let below = self
            .menu_room
            .borrow()
            .get(&menu)
            .map(|placed| placed.below);
        let room = below.unwrap_or(px(MENU_MAX_HEIGHT)) - px(MENU_DROP + MENU_GAP);
        self.menu_frame(width)
            .debug_selector(move || selector.to_owned())
            .max_h(room.min(px(MENU_MAX_HEIGHT)).max(px(0.)))
            .child(list)
    }

    /// A row of a dropdown, with the wash while it is the one picked.
    pub(super) fn menu_row(&self, id: SharedString, on: bool) -> Stateful<Div> {
        let theme = &self.look.theme;
        let hover = theme.active;
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(move || selector)
            .px_2()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(corners::SMALL))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .when(on, |el| el.bg(rgb(theme.primary_wash())))
    }

    pub(super) fn render_state_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let picked = StateChoice::of(self.filters);
        self.drop_down(
            Menu::State,
            crate::motion::pop_in(
                self.menu_frame(200.)
                    .children(StateChoice::ALL.into_iter().map(|choice| {
                        self.menu_row(
                            format!("orchestrator-state-{}", choice.id()).into(),
                            choice == picked,
                        )
                        .child(choice.label())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            choice.apply(&mut this.filters);
                            this.menu = None;
                            this.refresh_rows();
                            cx.notify();
                        }))
                    })),
                "orchestrator-state-menu",
            ),
            cx,
        )
    }

    /// What the Projects menu says of organization `repo` before its
    /// catalog is read: why its read failed, and whether it did, else that
    /// it is reading. A later read that succeeds replaces it.
    pub(super) fn catalog_note(&self, repo: usize) -> (bool, String) {
        let failed = self.repos.get(repo).and_then(|repo| {
            repo.snapshot
                .sources
                .iter()
                .find_map(|status| match &status.state {
                    SyncState::Failed(error) => Some(failure_text(Provider::Azure, error)),
                    _ => None,
                })
        });
        match failed {
            Some(text) => (true, text),
            None => (false, fl!("orchestrator-browse-projects-loading")),
        }
    }

    pub(super) fn render_projects_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        let theme = &look.theme;
        let browsed = self
            .repos
            .iter()
            .filter(|repo| matches!(repo.remote, Some(RemoteScope::Repository(_))))
            .count();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut drawn = 0;
        let mut left_out = 0;
        let note = |text: String| {
            div()
                .px_2()
                .py_1()
                .text_size(look.small())
                .text_color(rgb(theme.muted))
                .child(text)
                .into_any_element()
        };
        for organization in self.organizations() {
            let repo = organization.repo;
            rows.push(
                div()
                    .px_2()
                    .h(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(look.icon("icons/azure-devops.svg", 12., theme.subtext()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(organization.name.to_owned()),
                    )
                    .child(
                        look.icon_button(
                            SharedString::from(format!("orchestrator-projects-refresh-{repo}")),
                            "icons/refresh.svg",
                            fl!(
                                "orchestrator-browse-projects-refresh",
                                organization = organization.name
                            ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_organization(repo);
                            cx.notify();
                        })),
                    )
                    .into_any_element(),
            );
            let Some(catalog) = organization.catalog else {
                let (failed, text) = self.catalog_note(repo);
                let selector = if failed {
                    "orchestrator-projects-failed"
                } else {
                    "orchestrator-projects-loading"
                };
                rows.push(
                    div()
                        .debug_selector(move || selector.to_owned())
                        .child(note(text))
                        .into_any_element(),
                );
                continue;
            };
            if catalog.projects.is_empty() {
                rows.push(note(fl!("orchestrator-browse-projects-empty")));
            }
            for project in &catalog.projects {
                if drawn >= self.browse_rows {
                    left_out += project.repositories.len();
                    continue;
                }
                rows.push(
                    div()
                        .px_2()
                        .pt_1()
                        .truncate()
                        .text_size(look.small())
                        .text_color(rgb(theme.muted))
                        .child(project.name.clone())
                        .into_any_element(),
                );
                for name in &project.repositories {
                    if drawn >= self.browse_rows {
                        left_out += 1;
                        continue;
                    }
                    drawn += 1;
                    let source = SourceKey {
                        provider: Provider::Azure,
                        host: azure::HOST.to_owned(),
                        repository: format!("{}/{}/{name}", catalog.organization, project.name),
                    };
                    let on = self.browsed(&source);
                    let id = format!("orchestrator-projects-repo-{}", source.repository);
                    rows.push(
                        self.menu_row(id.into(), on)
                            .pl_4()
                            .child(div().w(px(12.)).flex_none().when(on, |el| {
                                el.child(look.icon("icons/check.svg", 12., theme.primary()))
                            }))
                            .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_browsed(source.clone());
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
        if left_out > 0 {
            rows.push(
                self.menu_row("orchestrator-projects-more".into(), false)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("orchestrator-browse-projects-more", count = left_out))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_more_repositories();
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        if browsed >= MAX_BROWSED {
            rows.push(note(fl!(
                "orchestrator-browse-projects-full",
                count = MAX_BROWSED
            )));
        }
        self.drop_down(
            Menu::Projects,
            crate::motion::pop_in(
                self.bounded_menu(
                    Menu::Projects,
                    320.,
                    "orchestrator-projects-frame",
                    div()
                        .id("orchestrator-projects-list")
                        .min_h_0()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(PICK_GAP))
                        .children(rows),
                ),
                "orchestrator-projects-menu",
            ),
            cx,
        )
    }
}
