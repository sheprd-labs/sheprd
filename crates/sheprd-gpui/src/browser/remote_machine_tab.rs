//! Opening and drawing a tab for a virtual Mac on another computer, from
//! the frames its Sheprd makes ([`crate::machine::remote::watch`]).

use super::{Location, Store, Tab, TabId};
use crate::{
    SheprdWindow,
    i18n::fl,
    machine::remote::watch::{self, Watch},
    teleport::Host,
};
use gpui::{prelude::*, *};
use sheprd_control_protocol::protocol::tabs::{MachineButton, MachineModifier, TabOp};
use std::{collections::HashMap, time::Instant};

/// Each remote machine tab's watcher, by tab.
#[derive(Default)]
struct Watches {
    watches: HashMap<TabId, Watch>,
    /// Where each tab last drew its frame area, for clicks.
    areas: HashMap<TabId, Bounds<Pixels>>,
    /// Each tab's clicks and keys, sent one at a time in order.
    inputs: HashMap<TabId, watch::Inputs>,
}

impl Global for Watches {}

/// What the frame loop does next.
enum Step {
    Stop,
    Idle,
    Fetch(
        Box<(
            Host,
            String,
            sheprd_control_protocol::protocol::lend::LendRequest,
        )>,
    ),
}

impl SheprdWindow {
    /// The host `endpoint`'s connection and label, while the window has it.
    fn remote_machine_host(&self, endpoint: &str) -> Option<(Host, String)> {
        let endpoint = self.endpoints.iter().find(|known| known.id == endpoint)?;
        let host = Host::new(&endpoint.connection.target).ok()?;
        Some((host, endpoint.label.clone()))
    }

    pub(crate) fn render_remote_machine_tab(
        &mut self,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(Location::RemoteMachine { endpoint, name }) = &tab.location else {
            return div().into_any_element();
        };
        let id = tab.id;
        let label = self
            .endpoints
            .iter()
            .find(|known| known.id == *endpoint)
            .map_or_else(|| endpoint.clone(), |known| known.label.clone());
        let focus = cx.focus_handle();
        let watches = cx.default_global::<Watches>();
        let watch = watches
            .watches
            .entry(id)
            .or_insert_with(|| Watch::new(focus));
        watch.drawn = Instant::now();
        let start = !std::mem::replace(&mut watch.running, true);
        let frame = watch.frame.clone();
        let problem = watch.problem.clone();
        let slow = watch
            .took
            .filter(|took| *took > std::time::Duration::from_secs(1));
        let focus = watch.focus.clone();
        if start {
            self.watch_remote_machine(id, endpoint.clone(), name.to_string(), cx);
        }
        let theme = &self.theme;
        let picture = match frame {
            Some(frame) => img(frame)
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.muted))
                .child(fl!(
                    "machine-remote-connecting",
                    name = name.as_str(),
                    host = label.clone()
                ))
                .into_any_element(),
        };
        let banner = |text: String| {
            div()
                .absolute()
                .bottom(px(8.))
                .left(px(8.))
                .right(px(8.))
                .p(px(6.))
                .rounded(px(6.))
                .bg(rgb(theme.background))
                .text_color(rgb(theme.muted))
                .child(text)
        };
        div()
            .id(("remote-machine", u64::from(id)))
            .debug_selector(|| "remote-machine".into())
            .size_full()
            .relative()
            .track_focus(&focus)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if let Some(op) = key_input(&event.keystroke) {
                    cx.stop_propagation();
                    this.remote_machine_input(id, op, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.remote_machine_click(id, event, MachineButton::Left, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.remote_machine_click(id, event, MachineButton::Right, window, cx);
                }),
            )
            .child(picture)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        cx.default_global::<Watches>().areas.insert(id, bounds);
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top(px(8.))
                    .right(px(8.))
                    .px(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(theme.background))
                    .text_color(rgb(theme.muted))
                    .child(match slow {
                        Some(_) => fl!("machine-remote-badge-slow", host = label.clone()),
                        None => fl!("machine-remote-badge", host = label.clone()),
                    }),
            )
            .when_some(problem, |tab, problem| tab.child(banner(problem)))
            .into_any_element()
    }

    /// The frame loop of tab `id`, while the tab exists.
    fn watch_remote_machine(
        &mut self,
        id: TabId,
        endpoint: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                let step = this.update(cx, |view, cx| {
                    let exists = cx
                        .try_global::<Store>()
                        .and_then(|store| store.get(id))
                        .is_some();
                    let watches = cx.default_global::<Watches>();
                    if !exists {
                        watches.watches.remove(&id);
                        watches.areas.remove(&id);
                        watches.inputs.remove(&id);
                        return Step::Stop;
                    }
                    let Some(watch) = watches.watches.get(&id) else {
                        return Step::Stop;
                    };
                    if watch.drawn.elapsed() > watch::SHOWN {
                        return Step::Idle;
                    }
                    let request = watch.next(&name);
                    match view.remote_machine_host(&endpoint) {
                        Some((host, label)) => Step::Fetch(Box::new((host, label, request))),
                        None => Step::Idle,
                    }
                });
                match step {
                    Err(_) | Ok(Step::Stop) => break,
                    Ok(Step::Idle) => {
                        // Drawing the tab again starts a new loop.
                        cx.update(|cx| {
                            if let Some(watch) = cx.default_global::<Watches>().watches.get_mut(&id)
                            {
                                watch.running = false;
                            }
                        });
                        break;
                    }
                    Ok(Step::Fetch(fetch)) => {
                        let (host, label, request) = *fetch;
                        let started = Instant::now();
                        let fetched = cx
                            .background_executor()
                            .spawn(async move {
                                watch::fetch(&host, &label, request, &Default::default())
                            })
                            .await;
                        let took = started.elapsed();
                        let _ = this.update(cx, |_, cx| {
                            if let Some(watch) = cx.default_global::<Watches>().watches.get_mut(&id)
                            {
                                watch.update(fetched, took);
                            }
                            cx.notify();
                        });
                        cx.background_executor().timer(watch::GAP).await;
                    }
                }
            }
        })
        .detach();
    }

    fn remote_machine_click(
        &mut self,
        id: TabId,
        event: &MouseDownEvent,
        button: MachineButton,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let watches = cx.default_global::<Watches>();
        let (Some(watch), Some(area)) = (watches.watches.get(&id), watches.areas.get(&id)) else {
            return;
        };
        let focus = watch.focus.clone();
        let area = (
            f32::from(area.origin.x),
            f32::from(area.origin.y),
            f32::from(area.size.width),
            f32::from(area.size.height),
        );
        let point = (f32::from(event.position.x), f32::from(event.position.y));
        let screen = watch.screen;
        window.focus(&focus, cx);
        let Some((x, y)) = watch::screen_point(point, area, screen) else {
            return;
        };
        let Some(name) = remote_name(id, cx) else {
            return;
        };
        let count = u8::try_from(event.click_count.clamp(1, 3)).unwrap_or(1);
        self.remote_machine_input(
            id,
            TabOp::MachineClick {
                name,
                x,
                y,
                button,
                count,
            },
            cx,
        );
    }

    /// Queues `op` for tab `id`'s machine, and starts sending unless input
    /// is already on its way: one at a time, so it arrives in order.
    fn remote_machine_input(&mut self, id: TabId, op: TabOp, cx: &mut Context<Self>) {
        let pushed = cx
            .default_global::<Watches>()
            .inputs
            .entry(id)
            .or_default()
            .push(op);
        match pushed {
            Ok(true) => self.send_remote_input(id, cx),
            Ok(false) => {}
            Err(()) => {
                if let Some(watch) = cx.default_global::<Watches>().watches.get_mut(&id) {
                    watch.problem = Some(fl!("machine-remote-input-full"));
                }
                cx.notify();
            }
        }
    }

    /// Sends tab `id`'s queued input until none is left.
    fn send_remote_input(&mut self, id: TabId, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                let next = this.update(cx, |view, cx| {
                    let op = cx
                        .default_global::<Watches>()
                        .inputs
                        .get_mut(&id)
                        .and_then(watch::Inputs::next)?;
                    let target = cx
                        .try_global::<Store>()
                        .and_then(|store| store.get(id))
                        .and_then(|tab| tab.location.clone());
                    let Some(Location::RemoteMachine { endpoint, name }) = target else {
                        // The tab closed: what it queued goes with it.
                        cx.default_global::<Watches>().inputs.remove(&id);
                        return None;
                    };
                    let (host, label) = view.remote_machine_host(&endpoint)?;
                    Some((host, label, named(op, name.to_string())))
                });
                let Ok(Some((host, label, op))) = next else {
                    // Without a host to send to, the rest cannot go either.
                    let _ = this.update(cx, |_, cx| {
                        if let Some(inputs) = cx.default_global::<Watches>().inputs.get_mut(&id) {
                            while inputs.next().is_some() {}
                        }
                    });
                    break;
                };
                let problem = cx
                    .background_executor()
                    .spawn(async move { watch::input(&host, &label, op, &Default::default()) })
                    .await;
                let _ = this.update(cx, |_, cx| {
                    if let Some(watch) = cx.default_global::<Watches>().watches.get_mut(&id) {
                        watch.problem = problem;
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

fn remote_name(id: TabId, cx: &App) -> Option<String> {
    match cx.try_global::<Store>()?.get(id)?.location.as_ref()? {
        Location::RemoteMachine { name, .. } => Some(name.to_string()),
        _ => None,
    }
}

/// `op` aimed at machine `name`; keys come without one.
fn named(op: TabOp, name: String) -> TabOp {
    match op {
        TabOp::MachineType { text, .. } => TabOp::MachineType { name, text },
        TabOp::MachineKey { key, modifiers, .. } => TabOp::MachineKey {
            name,
            key,
            modifiers,
        },
        op => op,
    }
}

/// What a key pressed over the tab sends: text as typed, or a key with its
/// modifiers. Cmd-W and the app's other shortcuts with Cmd stay the app's.
fn key_input(keystroke: &Keystroke) -> Option<TabOp> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform {
        return None;
    }
    let mut held = Vec::new();
    if modifiers.control {
        held.push(MachineModifier::Control);
    }
    if modifiers.alt {
        held.push(MachineModifier::Option);
    }
    if modifiers.function {
        held.push(MachineModifier::Fn);
    }
    if let Some(key) = watch::key_name(&keystroke.key) {
        if modifiers.shift {
            held.push(MachineModifier::Shift);
        }
        return Some(TabOp::MachineKey {
            name: String::new(),
            key: key.to_owned(),
            modifiers: held,
        });
    }
    if held.is_empty()
        && let Some(text) = keystroke.key_char.clone().filter(|text| !text.is_empty())
    {
        return Some(TabOp::MachineType {
            name: String::new(),
            text,
        });
    }
    let key = keystroke.key.to_lowercase();
    (key.chars().count() == 1).then(|| {
        if modifiers.shift {
            held.push(MachineModifier::Shift);
        }
        TabOp::MachineKey {
            name: String::new(),
            key,
            modifiers: held,
        }
    })
}

#[cfg(test)]
mod tests;
