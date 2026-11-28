//! Laying out the sidebar: the two lists, their headings, and the drag handle
//! that resizes the panel. Render works from prepared state and the bounded
//! caches only.

use super::{
    DEVICE_FOOTER_HEIGHT, HOST_ARROW_WIDTH, STATUS_WIDTH, SidebarDrag,
    agents::Indicators,
    agents::groups::agents_group,
    agents_sort,
    cell::{Cell, Fold, RowContext, RowData, WorkspaceRow, layout_for},
    label_text, layout, line_height,
    reorder::{self, Plan},
    repository,
    row::{RowIcon, RowLift, RowTree},
    scaled, sidebar_width, sticky,
    tokens::{self, SpaceContext},
    visible_workspace_entries, wash,
    workspaces::{displayed_workspace_status, workspace_badge, workspace_label, workspace_note},
};
use crate::direction::Logical;
use crate::i18n::fl;
use crate::{
    Command, NavigationTarget, SheprdWindow, avatars::Checkout, config::AgentsPlacement,
    fonts::StyledFont,
};
use gpui::{prelude::*, *};

mod actions;
mod agent_rows;
mod device_tree;
mod edges;
mod fold_hosts;
mod header;
mod menu_overlay;
mod nested_agents;
mod sections;

pub(super) use header::header;
pub(crate) use nested_agents::NestedReveal;

impl SheprdWindow {
    pub(crate) fn expanded_sidebar_width(&self, window: &Window) -> f32 {
        sidebar_width(self.sidebar_width, f32::from(window.viewport_size().width))
    }

    pub(crate) fn render_sidebar(
        &self,
        indicators: Indicators,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let width = self.expanded_sidebar_width(window);
        let split = self.sidebar_split.unwrap_or(0.5).clamp(0.1, 0.9);
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let agents_panel = self.config.show_agents
            && self.config.agents_placement == AgentsPlacement::Panel
            && self.config.layout.mode.lists_workspaces();
        let (mut spaces, drag) = self.sidebar_worktrees(indicators, width, window, cx);
        spaces.style().flex_grow = Some(if agents_panel { split } else { 1. });
        div()
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .role(Role::Navigation)
            .aria_label(fl!("sidebar-label"))
            .relative()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .text_color(rgb(theme.foreground))
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_e(px(1.))
            .border_color(rgb(theme.active))
            .child(spaces.child(self.sidebar_actions(cx)))
            .when(agents_panel, |sidebar| {
                let mut agents = self.render_agent_section(indicators, width, window, cx);
                agents.style().flex_grow = Some(1. - split);
                sidebar
                    .child(self.sidebar_split_edge(theme, cx))
                    .child(agents)
            })
            .children(self.render_mod_sidebar_sections(cx))
            .map(|sidebar| {
                if self.device_footer_shown() {
                    sidebar.child(self.render_device_footer(cx))
                } else {
                    sidebar.child(self.render_sessions_anchor())
                }
            })
            .child(self.sidebar_width_edge(width, cx))
            .child(drag)
    }

    pub(crate) fn sidebar_worktrees(
        &self,
        indicators: Indicators,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Div, AnyElement) {
        let look = layout::for_config(&self.config, self.fill());
        let rows = layout_for(self.config.layout.mode);
        // The row a workspace menu was opened for keeps looking hovered while
        // the pointer is over the menu.
        let menu_target = self.workspace_menu_target();
        let layout = look.density;
        let content_x = look.content_x();
        let view = cx.entity().downgrade();
        let font = &self.config.sidebar;
        let spaces_custom = self.config.usage.inline
            && self.config.sidebar_layout.spaces != crate::config::SpaceLayout::default();
        let repository_gap = repository::gap(&self.config);
        let theme = &self.theme;
        // The field is the heading, so it takes the heading's face.
        self.sidebar_search.input.update(cx, |input, cx| {
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        // The Devices layout heads every device, even a lone one, and lists
        // its agents under it in place of its workspaces. Its own field
        // narrows that tree in place, so the sidebar's search, whose results
        // replace the list, gives way to it; one setting hides either.
        let devices = !self.config.layout.mode.lists_workspaces();
        // A hidden field keeps whatever it held, but never filters the list.
        let searchable = self.config.show_sidebar_search && !devices;
        let tree_search = self.config.show_sidebar_search && devices;
        let search = (searchable && self.sidebar_search.query().is_some()).then(|| {
            self.sidebar_search_results(self.sidebar_search_hits(cx), indicators, look, cx)
        });
        let mut spaces = div()
            .id("spaces-scroll")
            .debug_selector(|| "spaces-scroll".into())
            .role(Role::List)
            .aria_label(if devices {
                fl!("sidebar-devices-label")
            } else {
                fl!("sidebar-workspaces-label")
            })
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        sticky::settle(&self.sidebar_scroll[0]);
        spaces = spaces.track_scroll(&self.sidebar_scroll[0]);
        let multi = self.endpoints.len() > 1;
        let headed = multi || devices;
        let placement = self
            .config
            .show_agents
            .then_some(self.config.agents_placement);
        let nested_agents = placement == Some(AgentsPlacement::Nested) && !devices;
        let query = if tree_search {
            self.device_tree_query(cx)
        } else {
            String::new()
        };
        // The focused agent's row when agents nest under their workspaces.
        let mut nested_focus = None;
        let mut space_rows = 0usize;
        let mut highlighted = None;
        // Each host header's endpoint index and child position in the spaces
        // list, for the header pinned over it.
        let mut host_rows = Vec::new();
        // A lifted workspace row: each drop unit's rows in the spaces list, and
        // the move every gap makes, for the pointer handler below.
        let mut drop_rows = Vec::new();
        let mut drop_requests = Vec::new();
        let mut drop_dragged = 0;
        let now = crate::motion::now();
        // A lifted device folds the list to its headers.
        let device_plan = headed.then(|| self.device_plan(devices, &query)).flatten();
        let mut sliding = device_plan.as_ref().is_some_and(|plan| plan.sliding);
        let worktree_glyph = self.config.worktrees.glyph(theme);
        // A lifted device folds the list to headers alone, and search results
        // are not arranged by group.
        let grouped = headed && device_plan.is_none() && query.is_empty() && self.device_tags.any();
        let tag_filter = self.device_tag_filter.as_deref();
        let mut section = None;
        // Sections are headings, so what lists under them steps in, with
        // each host's arrow below its section's dot.
        let heading_look = look;
        let look = layout::SidebarLook {
            group_indent: if grouped {
                scaled(font, HOST_ARROW_WIDTH) + look.density.host_gap()
            } else {
                0.
            },
            ..look
        };
        for (endpoint_index, endpoint) in self.displayed_devices() {
            if !self.device_visible(&endpoint.id)
                || (devices && !self.device_listed(endpoint_index, &query))
            {
                continue;
            }
            if grouped {
                let own = self.device_tags.section(&endpoint.id, tag_filter);
                if !section.is_some_and(|last| super::tag_groups::same_section(last, own)) {
                    section = Some(own);
                    let count = self.tag_section_count(own);
                    spaces = spaces.child(self.tag_section(own, count, heading_look, cx));
                    space_rows += 1;
                }
                if self.device_tag_folded(&endpoint.id) {
                    continue;
                }
            }
            let selected = endpoint_index == self.selected_endpoint;
            let endpoint_id = endpoint.id.clone();
            let mark = wash::HostMark::resolve(
                &self.config.sidebar_style,
                &endpoint.label,
                selected,
                theme,
            );
            if headed {
                host_rows.push((endpoint_index, space_rows));
                let header = self.host_row(endpoint, selected, look, width, false, cx);
                spaces = self.place_host_row(
                    spaces,
                    header,
                    (endpoint_index, space_rows),
                    device_plan.as_ref(),
                    cx,
                );
                space_rows += 1;
            }
            if device_plan.is_some() {
                continue;
            }
            if devices {
                let (list, rows, focused) =
                    self.append_device_agents(spaces, endpoint_index, indicators, look, width, cx);
                spaces = list;
                if let Some(row) = focused {
                    highlighted = Some(space_rows + row);
                }
                space_rows += rows;
                continue;
            }
            let row_cx = RowContext {
                indicators,
                font,
                theme,
                look,
                width,
                nest: if multi { look.nest_indent() } else { 0. },
                mark,
                host: (multi && endpoint_id != crate::endpoint::LOCAL)
                    .then_some(endpoint.label.as_str()),
            };
            let live = if selected { &self.live } else { &endpoint.live };
            // Herdr 1's workspaces, then those the app keeps for Herdr 2.
            let sessions = self.sidebar_sessions(endpoint_index, cx);
            if sessions.is_empty() {
                continue;
            }
            let snapshot = live.snapshot.as_deref();
            let silenced = snapshot
                .map(|snapshot| {
                    crate::notification_mutes::Mutes::silenced_in(cx, endpoint, snapshot)
                })
                .unwrap_or_default();
            let pr_view = self.endpoint_pr_view(endpoint_index);
            let collapsed_repos = if endpoint_index == 0 {
                &self.collapsed_repos
            } else {
                &endpoint.collapsed_repos
            };
            let kept_front = self.kept_in_front();
            for (keeper, session) in sessions.keyed() {
                let front = selected
                    .then(|| crate::session_model::Sessions::front_of(keeper, session, kept_front))
                    .flatten();
                let workspaces = &session.workspaces;
                let entries = visible_workspace_entries(workspaces, collapsed_repos);
                let drag = self.workspace_drag.as_ref().filter(|drag| {
                    selected && drag.keeper == keeper && drag.previewing(workspaces)
                });
                let floating = drag.is_some_and(|drag| drag.floating());
                let plan = drag.and_then(|drag| Plan::new(workspaces, &drag.workspace));
                // Each unit's shift for the drop in preview, from the heights the
                // last frame laid out: shifting moves rows, never resizes them.
                let base = space_rows;
                let shifts = plan.as_ref().map_or_else(Vec::new, |plan| {
                    drop_requests = (0..=plan.len())
                        .map(|slot| plan.request(workspaces, slot))
                        .collect();
                    drop_dragged = plan.dragged();
                    let mut heights = vec![0.; plan.len()];
                    for (position, entry) in entries.iter().enumerate() {
                        if let (Some(unit), Some(bounds)) = (
                            plan.unit_of(entry.0),
                            self.sidebar_scroll[0].bounds_for_item(base + position),
                        ) {
                            heights[unit] += f32::from(bounds.size.height);
                            if !entry.1 {
                                heights[unit] += repository_gap;
                            }
                        }
                    }
                    let slot = drag
                        .and_then(|drag| drag.target.as_ref())
                        .map_or(plan.dragged(), |target| target.slot);
                    crate::reorder::preview(&heights, plan.dragged(), slot)
                });
                // A child closes the group when no child follows it.
                let closes: Vec<bool> = (0..entries.len())
                    .map(|position| {
                        entries[position].1 && !entries.get(position + 1).is_some_and(|next| next.1)
                    })
                    .collect();
                // Gap separates one repository from the next, not a parent from
                // the linked worktrees under it. Reset per host.
                let mut previous_indented = None;
                for (position, (index, indented, group)) in entries.into_iter().enumerate() {
                    if multi && endpoint.collapsed {
                        break;
                    }
                    let workspace = &workspaces[index];
                    let linked = crate::worktree_identity::is_linked(workspace);
                    let focused = front == Some(workspace.id.as_str());
                    if focused {
                        highlighted = Some(space_rows);
                    }
                    let unit = plan.as_ref().and_then(|plan| plan.unit_of(index));
                    // The lifted row and the rows it carries, such as its group's
                    // children, follow the pointer together while it floats.
                    let carried = unit.is_some_and(|unit| unit == drop_dragged) && floating;
                    let shift = match (drag, unit) {
                        (Some(drag), Some(_)) if carried => {
                            drag.pin(&workspace.id, f32::from(drag.offset()), now);
                            drag.offset()
                        }
                        (Some(drag), Some(unit)) => {
                            let (at, moving) = drag.slide(&workspace.id, shifts[unit], now);
                            sliding |= moving;
                            px(at)
                        }
                        _ => px(0.),
                    };
                    if let Some(unit) = unit {
                        drop_rows.push((unit, space_rows, shift));
                    }
                    space_rows += 1;
                    let id = workspace.id.clone();
                    let closed_project = self.is_closed_project_row(&endpoint_id, &id, cx);
                    let press_id = id.clone();
                    let context_id = id.clone();
                    let hover_id = id.clone();
                    let context_endpoint = endpoint_id.clone();
                    let navigate_endpoint = endpoint_id.clone();
                    let collapse_endpoint = endpoint_id.clone();
                    let grouped = group.is_some() || indented;
                    let tree = match (indented, closes[position]) {
                        (false, _) => RowTree::None,
                        (true, false) => RowTree::Child,
                        (true, true) => RowTree::LastChild,
                    };
                    let fold_toggle = group.clone().map(|key| (endpoint_id.clone(), key));
                    let fold = group.map(|key| Fold {
                        id: SharedString::from(format!("collapse-{endpoint_id}-{id}")).into(),
                        index,
                        collapsed: collapsed_repos.contains(&key),
                        toggle: Box::new(cx.listener(move |this, _, _, cx| {
                            this.toggle_repository(&collapse_endpoint, &key, cx);
                        })),
                    });
                    let folds = fold.as_ref().map(|fold| fold.collapsed).zip(fold_toggle);
                    let label = workspace_label(workspace, indented);
                    let shown_status = if self.config.usage.inline {
                        displayed_workspace_status(workspaces, workspace, collapsed_repos)
                    } else {
                        workspace.status
                    };
                    let muted = silenced.workspace(&workspace.id);
                    let lines = if spaces_custom {
                        tokens::space_rows(
                            &self.config.sidebar_layout.spaces,
                            SpaceContext {
                                label,
                                branch: workspace.branch.as_deref(),
                                status: shown_status,
                                ahead_behind: workspace.ahead_behind,
                                tokens: &workspace.tokens,
                                indented,
                            },
                        )
                    } else {
                        Vec::new()
                    };
                    let gap = match previous_indented.replace(indented) {
                        Some(_) if !indented => repository_gap,
                        _ => 0.,
                    };
                    let removing_row = selected
                        && self.live.status.is_connected()
                        && snapshot.is_some_and(|snapshot| {
                            self.removal.as_ref().is_some_and(|removal| {
                                removal.pending_for(
                                    (self.selection_epoch, endpoint.generation),
                                    &snapshot.boot_id,
                                    &workspace.id,
                                )
                            })
                        });
                    // Lines under the row start under its label column, clear of
                    // the status dot.
                    let under_left = content_x
                        + row_cx.nest
                        + if indented {
                            layout.child_indent() + indicators.width(font) - STATUS_WIDTH
                        } else {
                            0.
                        }
                        + indicators.width(font)
                        + layout.gap();
                    let note = workspace_note(
                        workspace,
                        crate::worktree_notes::Notes::of(cx),
                        &endpoint.id,
                    )
                    .map(SharedString::from);
                    let note_line = note.clone().map(|note| {
                        let endpoint = endpoint_id.clone();
                        let workspace = id.clone();
                        super::note_line::element(
                            &format!("{endpoint_id}-{id}"),
                            note,
                            (under_left, content_x),
                            font,
                            theme,
                        )
                        .on_click(cx.listener(
                            move |this, event: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                this.edit_worktree_note(
                                    &endpoint,
                                    &workspace,
                                    event.position(),
                                    window,
                                    cx,
                                );
                            },
                        ))
                    });
                    // A child's trunk runs on past its note and ports to the next
                    // sibling, as it runs through the row itself.
                    let trunk = rows.trunk_below(&format!("{endpoint_id}-{id}"), tree, &row_cx);
                    let ports = (self.config.show_listening_ports && !removing_row)
                        .then(|| self.listed_ports(endpoint_index, &workspace.id))
                        .flatten()
                        .map(|listed| {
                            div()
                                .debug_selector(|| format!("ports-{endpoint_id}-{id}"))
                                .h(px(line_height(font)))
                                .flex_none()
                                .w_full()
                                .min_w_0()
                                .ps(px(under_left))
                                .pe(px(content_x))
                                .text_size(px((font.size * 0.85).round()))
                                .child(crate::listening_ports::chips(
                                    listed,
                                    (&endpoint_id, &workspace.id),
                                    &self.tunnels,
                                    theme,
                                    (font.size * 0.85).round(),
                                    cx,
                                ))
                        });
                    let element = Cell::new(
                        rows,
                        RowData::Workspace(WorkspaceRow {
                            workspace,
                            label,
                            tree,
                            repository: self
                                .config
                                .show_repository_icons
                                .then(|| repository::name(workspace))
                                .flatten(),
                            // A child hangs from the main checkout on the guide
                            // lines; one standing alone has no line to hang from,
                            // so it takes the branch glyph where the owner's
                            // avatar sat.
                            icon: if let Some(glyph) =
                                worktree_glyph.filter(|_| linked && !indented)
                            {
                                RowIcon::Worktree(glyph)
                            } else if indented || !self.config.show_repository_icons {
                                RowIcon::None
                            } else if workspace.branch.is_none() {
                                // Not a Git checkout, so no mark that reads as one.
                                RowIcon::Blank
                            } else {
                                let target = &endpoint.connection.target;
                                let image = self
                                    .avatars
                                    .as_ref()
                                    .zip(Checkout::of(endpoint_index == 0, target, workspace))
                                    .and_then(|(avatars, checkout)| avatars.image(checkout));
                                let name = repository::name(workspace).unwrap_or(label);
                                RowIcon::repository(name, image, theme)
                            },
                            fold,
                            grouped,
                            badge: workspace_badge(
                                workspace,
                                pr_view.as_ref().map(|view| (&self.menu.pr_cache, view)),
                                (
                                    (endpoint_index == 0).then_some(&self.git),
                                    self.config.sidebar_style.change_counts,
                                ),
                                (&self.teleport_marks, &endpoint.id),
                                (note.as_deref(), muted),
                                theme,
                            ),
                            removing: removing_row,
                            status: shown_status,
                            lines,
                        }),
                        &row_cx,
                    )
                    .selected(focused)
                    .highlighted(selected && menu_target == Some(id.as_str()))
                    .lift(match (carried, floating) {
                        (true, _) => RowLift::Lifted,
                        (false, true) => RowLift::Passed,
                        (false, false) => RowLift::Resting,
                    })
                    .row()
                    .when(
                        self.config.layout.mode == crate::config::LayoutMode::Minimal && gap > 0.,
                        |row| row.child(repository::separator(label, gap, content_x, theme)),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if closed_project {
                                this.open_closed_project_menu(
                                    &context_endpoint,
                                    &context_id,
                                    event.position,
                                    window,
                                    cx,
                                );
                                return;
                            }
                            if this.navigate_endpoint(
                                &context_endpoint,
                                NavigationTarget::Workspace(&context_id),
                                cx,
                            ) {
                                this.open_workspace_menu(&context_id, event.position, window, cx);
                                this.menu.opening_right_click =
                                    this.menu.page == Some(crate::menu::Page::Workspace);
                            }
                        }),
                    )
                    .when(
                        self.menu.page == Some(crate::menu::Page::Workspace),
                        |row| {
                            row.child(self.workspace_menu_overlay(
                                endpoint_id.clone(),
                                id.clone(),
                                cx,
                            ))
                        },
                    )
                    .id(SharedString::from(format!("workspace-{endpoint_id}-{id}")))
                    .map(|row| {
                        let name = super::accessibility::workspace_name(
                            label,
                            linked,
                            shown_status,
                            muted,
                            row_cx.host,
                        );
                        self.workspace_item(row, name, focused, folds, cx)
                    })
                    .when(multi, |row| {
                        row.debug_selector(|| format!("workspace-{endpoint_id}-{id}"))
                    })
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        let selected =
                            this.activate_sidebar_workspace(&navigate_endpoint, &id, window, cx);
                        window.focus(&this.focus, cx);
                        // The first click of a double-click has already selected
                        // the row, so the second only opens the rename dialog its
                        // menu's Rename opens, which takes focus. The fold chevron
                        // stops its clicks, so folding fast never renames.
                        if selected && !closed_project && event.click_count() == 2 {
                            this.open_workspace_dialog_for(
                                &id,
                                crate::menu::WorkspaceAction::Rename,
                                event.position(),
                                window,
                                cx,
                            );
                        }
                    }))
                    // Only the selected endpoint's rows arm the hover menu:
                    // another endpoint's menu would have to select it first, and
                    // resting the pointer must not switch which daemon is shown.
                    // The feature is opt-in, so rows stay unarmed without it.
                    .when(
                        selected && !closed_project && self.config.features.sidebar_hover_menu,
                        |row| {
                            row.on_hover(cx.listener(move |this, hovered: &bool, window, _| {
                                this.hover_workspace(&hover_id, *hovered, window);
                            }))
                        },
                    )
                    // Holding a press lifts the row for reordering. Another
                    // endpoint's rows would have to select it first.
                    .when(selected && !closed_project, |row| {
                        row.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                if this.config.features.sidebar_hover_menu {
                                    this.press_hovered_workspace(&press_id, event.position);
                                }
                                if event.click_count == 1 {
                                    this.press_workspace(&press_id, event.position, cx);
                                }
                            }),
                        )
                    })
                    .when(carried, |row| row.cursor_grabbing());
                    let indent = layout.child_indent() * if indented { 2. } else { 1. };
                    let agent_rows = if nested_agents {
                        let (rows, focused) = self.nested_agent_rows(
                            endpoint_index,
                            &workspace.id,
                            (&row_cx, indent),
                            cx,
                        );
                        nested_focus = nested_focus.or(focused);
                        rows
                    } else {
                        Vec::new()
                    };
                    // The note, ports and nested agents ride under their row as one
                    // list item, so the drop preview still measures one height per
                    // workspace.
                    let element = if note_line.is_none() && ports.is_none() && agent_rows.is_empty()
                    {
                        element
                            .when(gap > 0., |row| row.mt(px(gap)))
                            .when(shift != px(0.), |row| row.top(shift))
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w_full()
                            .relative()
                            .when(gap > 0., |unit| unit.mt(px(gap)))
                            .when(shift != px(0.), |unit| unit.top(shift))
                            .child(element)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .w_full()
                                    .relative()
                                    .children(note_line)
                                    .children(ports)
                                    .children(agent_rows)
                                    // Last, so a selected nested agent's fill does
                                    // not hide it, as a row draws its guide over
                                    // its own fill.
                                    .children(trunk),
                            )
                            .into_any_element()
                    };
                    spaces = if carried {
                        // Painted last so it floats over the rows it passes, while
                        // its layout slot keeps the others' positions stable.
                        spaces.child(deferred(element).with_priority(1))
                    } else {
                        spaces.child(element)
                    };
                }
            }
        }
        if sliding {
            window.request_animation_frame();
        }
        let mut headers = if headed {
            self.host_headers(&host_rows)
        } else {
            Vec::new()
        };
        if let Some(row) = self.sidebar_pin_reveal.take()
            && self.uncover_revealed(row, &headers)
        {
            headers = self.host_headers(&host_rows);
        }
        // Rows under the pinned header are hidden, so a reveal must clear it.
        let cover = px(sticky::cover(&headers));
        // Search results stand in for the spaces list, so no nested row paints.
        self.reveal_nested_agent(nested_focus.filter(|_| search.is_none()), cover, window);
        // Folded headers slide under a pinned one, so none is pinned then,
        // nor in the frame that unfolds, whose header places are the folded
        // list's; the frame after pins from the unfolded one.
        let unfolding = self.sidebar_folded.replace(device_plan.is_some()) && device_plan.is_none();
        if unfolding {
            window.request_animation_frame();
        }
        let pinned_host = sticky::pinned(&headers)
            .filter(|_| device_plan.is_none() && !unfolding)
            .and_then(|pinned| self.pinned_host_row(pinned, look, width, cx));
        self.reveal_sidebar_row(0, highlighted, cover, headed, window);
        let spaces = div()
            .debug_selector(|| "spaces-section".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            // With search on, the heading is its field. Beside it,
            // the button that collapses every host.
            .child(
                div()
                    .row()
                    .items_center()
                    .child(
                        searchable
                            .then_some(super::search::Heading::Spaces)
                            .or(tree_search.then_some(super::search::Heading::DeviceTree))
                            .and_then(|heading| self.search_heading(heading, look, window, cx))
                            .unwrap_or_else(|| {
                                let (key, label) = if devices {
                                    ("devices", fl!("sidebar-header-devices"))
                                } else {
                                    ("spaces", fl!("sidebar-header-spaces"))
                                };
                                header(key, &label, font, theme, look)
                            })
                            .flex_1()
                            .min_w_0(),
                    )
                    .children(self.fold_hosts_button(look, cx)),
            )
            .children(headed.then(|| self.tag_switch(cx)).flatten())
            // The wrapper clips the pinned header as the next host's
            // pushes it up, so it never paints over the title above.
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .map(|list| match search {
                        Some(results) => list.child(results),
                        None => list
                            .child(spaces)
                            .children(pinned_host.map(crate::mark_layer::covering)),
                    }),
            );
        let drag = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                // Capture globally so dragging continues outside the narrow divider,
                // and terminal handlers never receive the resize gesture's release.
                let moving = view.clone();
                let device_drop = device_plan.map(|plan| plan.drop);
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moving.update(cx, |this, cx| {
                            let scroll = this.sidebar_scroll[0].clone();
                            if this.move_workspace_drag(
                                event.position,
                                event.pressed_button == Some(MouseButton::Left),
                                |lift| {
                                    reorder::resolve(
                                        &drop_rows,
                                        &drop_requests,
                                        drop_dragged,
                                        &scroll,
                                        lift,
                                    )
                                },
                                cx,
                            ) || this.move_device_drag(
                                event.position,
                                event.pressed_button == Some(MouseButton::Left),
                                device_drop.as_ref(),
                                cx,
                            ) {
                                cx.stop_propagation();
                            } else if let Some(drag) = this.sidebar_drag {
                                match drag {
                                    SidebarDrag::Width { start, width } => {
                                        this.sidebar_width = Some(sidebar_width(
                                            // The edge is on the sidebar's end.
                                            Some(
                                                width
                                                    + crate::direction::toward_end(
                                                        f32::from(event.position.x) - start,
                                                    ),
                                            ),
                                            f32::from(window.viewport_size().width),
                                        ));
                                    }
                                    SidebarDrag::Split => {
                                        let footer = if this.device_footer_shown() {
                                            DEVICE_FOOTER_HEIGHT
                                        } else {
                                            0.
                                        };
                                        let height =
                                            (f32::from(bounds.size.height) - 6. - footer).max(1.);
                                        this.sidebar_split = Some(
                                            ((f32::from(event.position.y - bounds.origin.y) - 3.)
                                                / height)
                                                .clamp(0.1, 0.9),
                                        );
                                    }
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }
                        });
                    }
                });
                let released = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        let _ = released.update(cx, |this, cx| {
                            // A lifted row's release is its drop, not a click.
                            if this.release_workspace_drag(cx) || this.release_device_drag(cx) {
                                cx.stop_propagation();
                            } else if this.sidebar_drag.take().is_some() {
                                this.save_chrome();
                                cx.stop_propagation();
                                cx.notify();
                            }
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();
        (spaces, drag.into_any_element())
    }
}
