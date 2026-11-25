use super::*;
use crate::{
    config::status_bar::Detail,
    system_load::{Gpu, Memory, Sample},
    usage::Host,
};

fn sample() -> Sample {
    Sample {
        cpu: Some(12.),
        memory: Some(Memory {
            used: 4 << 30,
            total: 16 << 30,
        }),
        gpu: Some(Gpu {
            busy: Some(45.),
            memory: Some(Memory {
                used: 6 << 30,
                total: 24 << 30,
            }),
            count: 1,
        }),
        ..Sample::default()
    }
}

/// A device's row is sent what the classic row draws: CPU and memory, and
/// the GPU with its video memory while the sidebar shows it, with their
/// graphs once `sidebar_load` asks for the lines that draw them.
#[gpui::test]
fn a_device_carries_its_load_lines_and_its_gpu(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.config.show_system_load = true;
        view.config.show_sidebar_gpu_load = true;
        view.system_load.record(Host::Local, sample());
        let load = |view: &SheprdWindow, cx: &App| {
            view.hosted_state(cx).main["devices"][0]["load"].clone()
        };
        // Compact unless set otherwise: shares alone, and nothing to draw graphs from.
        assert_eq!(view.config.sidebar_load, Detail::Compact);
        for detail in [Detail::Compact, Detail::Detailed] {
            view.config.sidebar_load = detail;
            let lines = detail == Detail::Detailed;
            let device = view.hosted_state(cx).main["devices"][0].clone();
            assert_eq!(device["load_lines"], lines, "{detail:?}");
            let load = &device["load"];
            assert_eq!(load["cpu"].as_f64(), Some(12.), "{detail:?}");
            assert_eq!(load["memory"].as_f64(), Some(25.), "{detail:?}");
            assert_eq!(load["gpu"]["busy"].as_f64(), Some(45.), "{detail:?}");
            assert_eq!(load["gpu"]["memory"].as_f64(), Some(25.), "{detail:?}");
            // Sparklines and meters only for the lines that draw them.
            assert_eq!(load["history"].is_array(), lines, "{detail:?}");
            assert_eq!(load["meter"].is_object(), lines, "{detail:?}");
            assert_eq!(load["gpu"]["history"].is_array(), lines, "{detail:?}");
            assert_eq!(load["gpu"]["meter"].is_object(), lines, "{detail:?}");
        }

        // Each setting works without the other, as on the classic row.
        view.config.show_sidebar_gpu_load = false;
        let alone = load(view, cx);
        assert_eq!(alone["cpu"].as_f64(), Some(12.));
        assert!(alone.get("gpu").is_none());

        view.config.show_system_load = false;
        view.config.show_sidebar_gpu_load = true;
        let gpu = load(view, cx);
        assert!(gpu.get("cpu").is_none());
        assert_eq!(gpu["gpu"]["busy"].as_f64(), Some(45.));

        view.config.show_sidebar_gpu_load = false;
        assert!(load(view, cx).is_null());
    });
}
