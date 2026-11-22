//! A deduplicated attention count across all windows, with a QA preview: the
//! Dock badge on macOS and the taskbar overlay on Windows. Agents in a
//! workspace or tab the user muted are not counted, nor those whose
//! Overview row the user settled or snoozed, while that hold lasts. Herdr 2
//! agents count too: every window shares this computer's herdr-core, so
//! each terminal counts once however many windows show it.

use crate::{
    endpoint::Endpoint,
    notification_mutes::{Daemon, Mutes, Silenced},
    overview::{Hold, Key, Marks, State},
};
use gpui::{App, BorrowAppContext, Global, WindowId};
use sheprd_backend::TerminalId;
use sheprd_client::ConnectTarget;
use sheprd_client::protocol::{AgentStatus, ClientShellSnapshot};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::SystemTime,
};

/// One snapshot a window counts, and its agents the user put aside.
struct Shown {
    host: Option<String>,
    snapshot: Arc<ClientShellSnapshot>,
    /// Panes whose workspace or tab is muted, or whose Overview row is
    /// settled or snoozed: they need no attention here.
    aside: HashSet<String>,
}

/// A herdr-core terminal whose agent a window found needing the user:
/// blocked, or done, and not muted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CoreAgent {
    pub(super) terminal: TerminalId,
    pub(super) status: AgentStatus,
    /// Whether the Overview lists the agent as a row, so a mark can hold
    /// it. The stand-in shell's agent has none.
    pub(super) listed: bool,
}

#[derive(Default)]
struct Contribution {
    /// The window's herdr-core terminals whose agent needs the user.
    core: HashSet<TerminalId>,
    /// What `core` was found from: the window's core agents, how many lists
    /// of terminals the core had sent, and the marks' revision.
    core_from: (Vec<CoreAgent>, u64, u64),
    /// When the first snooze that left a core agent out ends.
    core_wakes: Option<SystemTime>,
    snapshots: Vec<Shown>,
    /// The mutes' and the Overview marks' revisions the panes aside were
    /// found under.
    revisions: (u64, u64),
    /// When the first snooze that put a pane aside ends, so its agent then
    /// counts again though nothing else changed.
    wakes: Option<SystemTime>,
}

impl Contribution {
    /// Takes the window's `snapshots`, each with its host and the endpoint
    /// it came from, under the app's `mutes` and Overview `marks` at `now`,
    /// empty or not: lifting the last mute is a change too. Returns whether
    /// the count may have changed.
    fn update<'a>(
        &mut self,
        snapshots: impl Iterator<Item = (Option<&'a str>, &'a Arc<ClientShellSnapshot>, &'a Endpoint)>
        + Clone,
        mutes: Option<&Mutes>,
        marks: Option<&Marks>,
        now: SystemTime,
    ) -> bool {
        let revisions = (
            mutes.map_or(0, Mutes::revision),
            marks.map_or(0, Marks::revision),
        );
        // Surface-only updates keep the same Arc. Do not rescan agents every tick.
        if self.revisions == revisions
            && self.wakes.is_none_or(|wakes| now < wakes)
            && self
                .snapshots
                .iter()
                .map(|shown| (shown.host.as_deref(), Arc::as_ptr(&shown.snapshot)))
                .eq(snapshots
                    .clone()
                    .map(|(host, snapshot, _)| (host, Arc::as_ptr(snapshot))))
        {
            return false;
        }
        self.revisions = revisions;
        let mut wakes = None;
        self.snapshots = snapshots
            .map(|(host, snapshot, endpoint)| {
                let mut aside = mutes
                    .filter(|mutes| !mutes.is_empty())
                    .map(|mutes| silent(&mutes.silenced(&Daemon::of(endpoint), snapshot), snapshot))
                    .unwrap_or_default();
                if let Some(marks) = marks {
                    aside.extend(held(marks, &endpoint.id, snapshot, now, &mut wakes));
                }
                Shown {
                    host: host.map(str::to_owned),
                    snapshot: Arc::clone(snapshot),
                    aside,
                }
            })
            .collect();
        self.wakes = wakes;
        true
    }
}

/// Whether an agent in `status` needs the user: finished, or waiting.
fn needs_user(status: AgentStatus) -> bool {
    matches!(status, AgentStatus::Done | AgentStatus::Blocked)
}

/// The panes of `snapshot`'s agents needing the user whose Overview row,
/// keyed under `endpoint` as the Overview keys it, `marks` hold at `now`
/// for the state the agent is in: the Overview lists none of them among the
/// rows to look at. Notes in `wakes` when the first of their snoozes ends.
fn held(
    marks: &Marks,
    endpoint: &str,
    snapshot: &ClientShellSnapshot,
    now: SystemTime,
    wakes: &mut Option<SystemTime>,
) -> Vec<String> {
    snapshot
        .agents
        .iter()
        .filter(|agent| needs_user(agent.agent_status))
        .filter(|agent| {
            let key = crate::overview::agent_key(endpoint, snapshot, agent);
            held_at(marks, &key, agent.agent_status, now, wakes)
        })
        .map(|agent| agent.pane_id.clone())
        .collect()
}

/// Whether `marks` hold `key`'s row at `now` for an agent in `status`, as
/// the Overview sorts it, noting in `wakes` when a snooze holding it ends.
fn held_at(
    marks: &Marks,
    key: &Key,
    status: AgentStatus,
    now: SystemTime,
    wakes: &mut Option<SystemTime>,
) -> bool {
    let hold = marks
        .get(key)
        .and_then(|mark| mark.hold(State::of(status), now));
    if let Some(Hold::Snoozed { until }) = hold {
        *wakes = Some(wakes.map_or(until, |wakes| wakes.min(until)));
    }
    hold.is_some()
}

/// The panes of `snapshot`'s agents that `silenced` keeps quiet.
fn silent(silenced: &Silenced<'_>, snapshot: &ClientShellSnapshot) -> HashSet<String> {
    if silenced.is_empty() {
        return HashSet::new();
    }
    snapshot
        .agents
        .iter()
        .filter(|agent| {
            silenced.notice(
                Some(&agent.workspace_id),
                Some(&agent.tab_id),
                Some(&agent.pane_id),
            )
        })
        .map(|agent| agent.pane_id.clone())
        .collect()
}

#[derive(Default)]
struct Badge {
    windows: HashMap<WindowId, Contribution>,
    published: Option<usize>,
    /// The locale the published count was described in: Windows' taskbar
    /// speaks the count, so a language switch publishes it again.
    published_in: Option<sheprd_i18n::Locale>,
    preview: bool,
    permission_requested: bool,
}

impl Global for Badge {}

impl Badge {
    fn publish(&mut self) {
        // Snapshot revisions are client-local. Agent state sequences are shared;
        // for the same sequence, an acknowledgement wins over a stale Done dot.
        let mut agents = HashMap::new();
        for Shown {
            host,
            snapshot,
            aside,
        } in self.windows.values().flat_map(|window| &window.snapshots)
        {
            for agent in &snapshot.agents {
                let attention = needs_user(agent.agent_status) && !aside.contains(&agent.pane_id);
                let current = agents
                    .entry((
                        host.as_deref(),
                        snapshot.boot_id.as_str(),
                        agent.pane_id.as_str(),
                    ))
                    .or_insert((agent.state_change_seq, attention));
                if agent.state_change_seq > current.0 {
                    *current = (agent.state_change_seq, attention);
                } else if agent.state_change_seq == current.0 {
                    current.1 &= attention;
                }
            }
        }
        let core: HashSet<&TerminalId> = self
            .windows
            .values()
            .flat_map(|window| &window.core)
            .collect();
        let count = (agents.values().filter(|(_, attention)| *attention).count() + core.len())
            .max(if self.preview { 2 } else { 0 });
        let locale = crate::i18n::shown_locale();
        if self.published != Some(count) || self.published_in != locale {
            // Asked on the first badge rather than at launch, so an app that
            // never needs attention never prompts.
            if count > 0 && !self.permission_requested {
                self.permission_requested = true;
                request_permission();
            }
            set_native(count);
            self.published = Some(count);
            self.published_in = locale;
        }
    }
}

pub(super) fn install(cx: &mut App) {
    cx.default_global::<Badge>().publish();
    cx.on_action(|action: &crate::actions::SetBadgePreview, cx| {
        let badge = cx.default_global::<Badge>();
        badge.preview = action.enabled;
        badge.publish();
    });
    cx.on_window_closed(|cx, _| {
        let open = cx.windows();
        #[cfg(windows)]
        crate::taskbar::button::retain(&open);
        let badge = cx.default_global::<Badge>();
        badge
            .windows
            .retain(|id, _| open.iter().any(|window| window.window_id() == *id));
        badge.publish();
    })
    .detach();
}

pub(super) fn sync(window: WindowId, endpoints: &[Endpoint], cx: &mut App) {
    let shown = endpoints
        .iter()
        .filter(|endpoint| endpoint.enabled && endpoint.live.status.is_connected())
        .filter_map(|endpoint| {
            let host = match &endpoint.connection.target {
                ConnectTarget::Ssh { target, .. } => Some(target.as_str()),
                ConnectTarget::Wsl { distro, .. } => Some(distro.as_str()),
                _ => None,
            };
            endpoint
                .live
                .snapshot
                .as_ref()
                .map(|snapshot| (host, snapshot, endpoint))
        });
    cx.default_global::<Badge>();
    // Leased out of the app, so the mutes and marks can be read beside it.
    cx.update_global::<Badge, _>(|badge, cx| {
        let changed = badge.windows.entry(window).or_default().update(
            shown,
            Mutes::of(cx),
            Marks::of(cx),
            SystemTime::now(),
        );
        if changed || badge.published_in != crate::i18n::shown_locale() {
            badge.publish();
        }
    });
}

/// Takes the window's herdr-core `agents` that need the user, found after
/// the core sent `lists` lists of terminals, leaving out each the Overview
/// lists whose row the user's marks hold. `key` names an agent's row from
/// what the core last listed: it is asked again only once the agents, the
/// lists, or the marks change, or a snooze ends, not on every tick.
pub(super) fn sync_core(
    window: WindowId,
    agents: Vec<CoreAgent>,
    lists: u64,
    key: impl Fn(&TerminalId) -> Option<Key>,
    cx: &mut App,
) {
    cx.default_global::<Badge>();
    cx.update_global::<Badge, _>(|badge, cx| {
        let marks = Marks::of(cx);
        let now = SystemTime::now();
        let contribution = badge.windows.entry(window).or_default();
        let from = (agents, lists, marks.map_or(0, Marks::revision));
        if contribution.core_from == from && contribution.core_wakes.is_none_or(|wakes| now < wakes)
        {
            return;
        }
        let mut wakes = None;
        let core = from
            .0
            .iter()
            .filter(|agent| {
                let held = agent.listed
                    && marks.is_some_and(|marks| {
                        key(&agent.terminal)
                            .is_some_and(|key| held_at(marks, &key, agent.status, now, &mut wakes))
                    });
                !held
            })
            .map(|agent| agent.terminal.clone())
            .collect();
        contribution.core_from = from;
        contribution.core_wakes = wakes;
        if contribution.core != core {
            contribution.core = core;
            badge.publish();
        }
    });
}

#[cfg(all(not(test), target_os = "macos"))]
fn set_native(count: usize) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
    NSApplication::sharedApplication(main_thread)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

#[cfg(all(not(test), windows))]
fn set_native(count: usize) {
    crate::taskbar::button::set_overlay(count);
}

// Headless tests must never initialize AppKit or change the test runner's Dock icon.
#[cfg(test)]
fn set_native(_: usize) {}

/// Once an app is registered with the notification center, macOS hides its
/// Dock badge unless the badge option was authorized. GPUI requests only alert
/// and sound when it posts a system notification, so the badge label would be
/// set but never drawn. This request asks for the union: whichever request
/// macOS answers first decides the permission, and a later request with other
/// options is not prompted for again.
#[cfg(all(not(test), target_os = "macos"))]
fn request_permission() {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError};
    use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};

    // The notification center aborts the process outside an app bundle,
    // which is how `cargo run` builds start.
    if NSBundle::mainBundle().bundleIdentifier().is_none() {
        return;
    }
    // Runs on a notification-center queue, not the UI thread.
    let completion = RcBlock::new(|granted: Bool, _: *mut NSError| {
        if !granted.as_bool() {
            tracing::info!("Dock badge permission was not granted");
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge,
            &completion,
        );
}

/// Windows asks no permission for a taskbar overlay.
#[cfg(all(not(test), windows))]
fn request_permission() {}

#[cfg(test)]
thread_local! {
    static PERMISSION_REQUESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// Headless tests must never register the test runner with the notification center.
#[cfg(test)]
fn request_permission() {
    PERMISSION_REQUESTS.set(PERMISSION_REQUESTS.get() + 1);
}

#[cfg(all(target_os = "macos", feature = "integration-test"))]
pub(super) fn verify_native() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let main_thread = MainThreadMarker::new().context("badge check requires the main thread")?;
    let tile = NSApplication::sharedApplication(main_thread).dockTile();
    let previous = tile.badgeLabel();
    for count in [2, 1, 0] {
        set_native(count);
        let actual = tile.badgeLabel().map(|label| label.to_string());
        let valid = if count > 0 {
            actual.as_deref() == Some(count.to_string().as_str())
        } else {
            actual.as_deref().is_none_or(str::is_empty)
        };
        if !valid {
            tile.setBadgeLabel(previous.as_deref());
            anyhow::bail!("native Dock badge did not reflect count={count}");
        }
    }
    tile.setBadgeLabel(previous.as_deref());
    eprintln!("BADGE native PASS: Dock count changed from 2 to 1 and cleared");
    Ok(())
}

#[cfg(test)]
mod tests;
