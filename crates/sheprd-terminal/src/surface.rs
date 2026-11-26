//! Incremental screen and input state shared by raw terminal transports.
use crate::{Damage, Emulator, Event, MouseReporting, Scroll};
use sheprd_backend::{
    ClientMouseKind, ClientPaneInputEvent, KeyReporting, Role, ScrollMetrics, SurfaceUpdate,
};
use std::time::{Duration, Instant};

const SYNC_TIMEOUT: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct Surface {
    keys: Option<KeyReporting>,
    reporting: Option<bool>,
    scroll: Option<(ScrollMetrics, u64)>,
    sync_since: Option<(Instant, u64)>,
    wraps: Vec<bool>,
}

impl Surface {
    /// The view's place in its history and what changed on screen, when
    /// either changed since the view was last told.
    pub fn push_view(&mut self, emulator: &mut Emulator, updates: &mut Vec<SurfaceUpdate>) {
        let scroll = (emulator.scroll_metrics(), emulator.revision());
        if self.scroll != Some(scroll) {
            self.scroll = Some(scroll);
            updates.push(SurfaceUpdate::Scroll {
                metrics: scroll.0,
                revision: scroll.1,
            });
        }
        let damaged = match emulator.take_damage() {
            Damage::Full(frame) => {
                updates.push(SurfaceUpdate::Frame(frame));
                true
            }
            damage if damage.is_empty() => false,
            Damage::Rows { rows, cursor } => {
                updates.push(SurfaceUpdate::Rows { cursor, rows });
                true
            }
        };
        if damaged {
            let wraps = emulator.wrapped_rows();
            if wraps != self.wraps {
                self.wraps = wraps.clone();
                updates.push(SurfaceUpdate::Wraps(wraps));
            }
        }
        // After the cells, so the images land on the screen they belong to.
        if let Some(scene) = emulator.take_graphics() {
            updates.push(SurfaceUpdate::Graphics(scene));
        }
    }

    fn release_stale_sync(&mut self, emulator: &mut Emulator) {
        if !emulator.sync_pending() {
            self.sync_since = None;
            return;
        }
        let ended = emulator.syncs_ended();
        let since = match self.sync_since {
            Some((since, seen)) if seen == ended => since,
            _ => self.sync_since.insert((Instant::now(), ended)).0,
        };
        if since.elapsed() >= SYNC_TIMEOUT {
            emulator.end_sync();
            self.sync_since = None;
        }
    }
    pub fn timeout(&self, timeout: Duration) -> Duration {
        self.sync_since.map_or(timeout, |(since, _)| {
            timeout.min((since + SYNC_TIMEOUT).saturating_duration_since(Instant::now()))
        })
    }

    pub fn updates(&mut self, emulator: &mut Emulator, ended: bool) -> Vec<SurfaceUpdate> {
        self.release_stale_sync(emulator);
        if ended {
            emulator.end_sync();
        }
        let mut updates: Vec<SurfaceUpdate> = emulator
            .take_events()
            .into_iter()
            .map(|event| match event {
                Event::Bell { .. } => SurfaceUpdate::Bell,
                Event::Clipboard(text) => SurfaceUpdate::Clipboard(text),
                Event::Title(title) => SurfaceUpdate::Title(title.unwrap_or_default()),
            })
            .collect();
        let modes = emulator.modes();
        let keys = KeyReporting {
            every_key: modes.reports_every_key(),
            releases: modes.reports_key_releases(),
        };
        if self.keys != Some(keys) {
            self.keys = Some(keys);
            updates.push(SurfaceUpdate::KeyReporting(keys));
        }
        let reporting = modes.mouse != MouseReporting::Off;
        if self.reporting != Some(reporting) {
            self.reporting = Some(reporting);
            updates.push(SurfaceUpdate::MouseReporting(reporting));
        }
        self.push_view(emulator, &mut updates);
        updates
    }
}

impl Emulator {
    pub fn input_bytes(&mut self, input: &ClientPaneInputEvent, role: Role) -> Vec<u8> {
        let bytes = self.encode(input);
        if bytes.is_empty() {
            if let ClientPaneInputEvent::Mouse { kind, lines, .. } = input {
                let lines = i32::from((*lines).max(1));
                match kind {
                    ClientMouseKind::ScrollUp => self.scroll(Scroll::Lines(lines)),
                    ClientMouseKind::ScrollDown => self.scroll(Scroll::Lines(-lines)),
                    _ => {}
                }
            }
            return Vec::new();
        }
        if role == Role::Observer {
            return Vec::new();
        }
        if !matches!(input, ClientPaneInputEvent::Mouse { .. }) && self.scroll_metrics().offset > 0
        {
            self.scroll(Scroll::Bottom);
        }
        bytes
    }
}
