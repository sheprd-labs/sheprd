//! The sidebar's drag handle: the invisible layer that carries a lifted row's
//! drop and resizes the panel, from press to release.

use super::super::{DEVICE_FOOTER_HEIGHT, SidebarDrag, reorder, sidebar_width};
use crate::SheprdWindow;
use crate::sidebar::device_drag::DeviceDrop;
use gpui::{prelude::*, *};

/// The layer over the whole sidebar that follows a pointer drag: a lifted
/// workspace or device drop first, then the width or split resize.
pub(super) fn drag_handle(
    view: WeakEntity<SheprdWindow>,
    drop_rows: Vec<(usize, usize, Pixels)>,
    drop_requests: Vec<Option<reorder::MoveBlock>>,
    drop_dragged: usize,
    device_drop: Option<DeviceDrop>,
) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            // Capture globally so dragging continues outside the narrow divider,
            // and terminal handlers never receive the resize gesture's release.
            let moving = view.clone();
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
    .size_full()
    .into_any_element()
}
