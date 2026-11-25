//! The GPU a layout that draws its own status bar and host rows is sent:
//! the Moonframe window, the default on macOS and Linux, showed none.
use super::*;
use crate::{
    config::status_bar::Detail, sidebar::layout_tests::fixture_window, system_load::gpu::Gpu,
};
use gpui::TestAppContext;

fn sample(busy: Option<f32>, vram: Option<Memory>) -> Sample {
    Sample {
        cpu: Some(10.),
        memory: Some(Memory {
            used: 4 << 30,
            total: 16 << 30,
        }),
        gpu: Some(Gpu {
            busy,
            memory: vram,
            count: 1,
        }),
        ..Sample::default()
    }
}

const VRAM: Memory = Memory {
    used: 6 << 30,
    total: 24 << 30,
};

#[gpui::test]
fn the_bar_state_carries_the_gpu_while_it_is_shown(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_system_load = true;
        view.config.show_gpu_load = true;
        view.config.status_bar.gpu_detail = Detail::Detailed;
        view.system_load
            .record(Host::Local, sample(Some(30.), Some(VRAM)));
        view.system_load
            .record(Host::Local, sample(Some(90.), Some(VRAM)));
        view
    });
    view.update(cx, |view, _| {
        let load = view.hosted_load();
        let gpu = &load["gpu"];
        assert_eq!(gpu["busy"].as_f64(), Some(90.));
        assert!((gpu["memory"].as_f64().unwrap() - 25.).abs() < 1e-6);
        // Past critical, the share takes the danger color.
        let danger = crate::sidebar::hosted::hex(view.theme.ink(view.theme.palette[1]));
        assert_eq!(gpu["busy_color"], danger.as_str());
        let history = gpu["history"].as_array().unwrap();
        assert_eq!(history.len(), HISTORY);
        assert!((history[HISTORY - 1]["value"].as_f64().unwrap() - 0.9).abs() < 1e-6);
        assert!((gpu["meter"]["used"].as_f64().unwrap() - 0.25).abs() < 1e-6);
        // CPU and memory stay beside it.
        assert_eq!(load["cpu"].as_f64(), Some(10.));

        view.config.status_bar.gpu_detail = Detail::Compact;
        let compact = view.hosted_load();
        assert!(compact["gpu"].get("history").is_none() && compact["gpu"].get("meter").is_none());

        // The GPU alone, as the classic bar shows it without CPU and memory.
        view.config.show_system_load = false;
        let alone = view.hosted_load();
        assert!(alone.get("cpu").is_none());
        assert_eq!(alone["gpu"]["busy"].as_f64(), Some(90.));

        view.config.show_gpu_load = false;
        assert!(view.hosted_load().is_null());
    });
}

#[gpui::test]
fn a_card_reporting_nothing_sends_no_gpu(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_system_load = true;
        view.config.show_gpu_load = true;
        view.system_load.record(Host::Local, sample(None, None));
        view
    });
    view.update(cx, |view, _| {
        assert!(view.hosted_load().get("gpu").is_none());
    });
}
