//! A host's load as data, for a layout that draws the status bar and the
//! sidebar's host rows itself: what [`SheprdWindow::render_system_load`],
//! [`SheprdWindow::render_gpu_load`] and the classic host row draw, with the
//! same sparklines, meters, shares and colors, and nothing when they draw
//! nothing.

use super::{
    HISTORY, Reading,
    render::{CPU_WARN, GPU_WARN, MEMORY_WARN, VRAM_WARN, severity},
};
use crate::{
    config::{Theme, status_bar::Detail},
    i18n::fl,
    sidebar::hosted::hex,
    window::SheprdWindow,
};
use serde_json::{Value, json};

impl SheprdWindow {
    /// The selected host's load, as the status bar shows it: see
    /// [`host_load`]. CPU and memory while `show_system_load` is on, the
    /// GPU while `show_gpu_load` is, each at its own detail.
    pub(crate) fn hosted_load(&self) -> Value {
        let Some(reading) = self
            .selected_host()
            .and_then(|host| self.system_load.get(&host))
        else {
            return Value::Null;
        };
        let bar = &self.config.status_bar;
        host_load(
            reading,
            &self.theme,
            self.config.show_system_load.then_some(bar.system_load),
            self.config.show_gpu_load.then_some(bar.gpu_detail),
        )
    }
}

/// A host's load, for the status bar or its row in the sidebar: `cpu` and
/// `memory` as percentages, their `cpu_color` and `memory_color`, and while
/// `load` is detailed the CPU `history` (oldest first, [`HISTORY`] bars,
/// `false` for those not sampled yet) and the memory `meter`. `null` while
/// hidden or before the host's first answer; `unavailable` when it never
/// answered. While `gpu_detail` shows the GPU and the host has a card,
/// `gpu` holds its line (see [`gpu`]); alone when `load` hides CPU and
/// memory.
pub(crate) fn host_load(
    reading: &Reading,
    theme: &Theme,
    load: Option<Detail>,
    gpu_detail: Option<Detail>,
) -> Value {
    let gpu = gpu_detail.and_then(|detail| gpu(reading, theme, detail));
    let Some(detail) = load else {
        return gpu.map_or(Value::Null, |gpu| json!({ "gpu": gpu }));
    };
    let Some(sample) = reading.latest() else {
        return match reading.error() {
            Some(_) => json!({ "unavailable": fl!("system-load-unavailable") }),
            None => Value::Null,
        };
    };
    let share = share(reading, theme);
    let memory = sample.memory.map(|memory| memory.percent());
    let mut load = json!({
        "cpu": sample.cpu,
        "cpu_color": sample.cpu.map(|cpu| share(cpu, CPU_WARN)),
        "cpu_label": fl!("system-load-cpu"),
        "label_color": hex(theme.muted),
        "memory": memory,
        "memory_color": memory.map(|used| share(used, MEMORY_WARN)),
        "memory_label": fl!("system-load-memory"),
    });
    if detail == Detail::Detailed {
        load["history"] = history(reading.history(), CPU_WARN, theme);
        load["track"] = hex(theme.active).into();
        if let Some(used) = memory {
            load["meter"] = meter(used, MEMORY_WARN, theme);
        }
    }
    if let Some(gpu) = gpu {
        load["gpu"] = gpu;
    }
    load
}

/// The GPU's line as the bar draws it (`render::gpu_line`): its `busy`
/// share and video `memory` (VRAM) share with their colors and labels, and
/// while `detail` is detailed the busy `history` and the VRAM `meter`.
/// None on a host without a card that reports either.
fn gpu(reading: &Reading, theme: &Theme, detail: Detail) -> Option<Value> {
    let gpu = reading.latest()?.gpu?;
    let memory = gpu.memory.map(|memory| memory.percent());
    if gpu.busy.is_none() && memory.is_none() {
        return None;
    }
    let share = share(reading, theme);
    let mut line = json!({
        "busy": gpu.busy,
        "busy_color": gpu.busy.map(|busy| share(busy, GPU_WARN)),
        "busy_label": fl!("system-load-gpu"),
        "label_color": hex(theme.muted),
        "memory": memory,
        "memory_color": memory.map(|used| share(used, VRAM_WARN)),
        "memory_label": fl!("system-load-video-memory"),
    });
    if detail == Detail::Detailed {
        line["track"] = hex(theme.active).into();
        if gpu.busy.is_some() {
            line["history"] = history(reading.gpu_history(), GPU_WARN, theme);
        }
        if let Some(used) = memory {
            line["meter"] = meter(used, VRAM_WARN, theme);
        }
    }
    Some(line)
}

/// A share's color: its severity's, or muted while the numbers are the last
/// good ones and a refresh fails.
fn share<'a>(reading: &Reading, theme: &'a Theme) -> impl Fn(f32, f32) -> String + 'a {
    let stale = reading.error().is_some();
    move |value, warn| {
        hex(if stale {
            theme.muted
        } else {
            severity(value, warn, theme, theme.foreground)
        })
    }
}

/// [`HISTORY`] sparkline bars, oldest first, `false` for those not sampled.
fn history(values: impl ExactSizeIterator<Item = f32>, warn: f32, theme: &Theme) -> Value {
    let empty = HISTORY.saturating_sub(values.len());
    std::iter::repeat_n(Value::Bool(false), empty)
        .chain(values.map(|value| {
            json!({
                "value": value / 100.,
                "color": hex(severity(value, warn, theme, theme.muted)),
            })
        }))
        .collect::<Vec<_>>()
        .into()
}

fn meter(used: f32, warn: f32, theme: &Theme) -> Value {
    json!({
        "used": used / 100.,
        "color": hex(severity(used, warn, theme, theme.muted)),
    })
}
