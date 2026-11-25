use super::*;
use crate::{
    system_load::{Gpu, Memory, Sample},
    usage::Host,
};
use sheprd_client::ConnectTarget;

fn sample(cpu: f32, busy: f32) -> Sample {
    Sample {
        cpu: Some(cpu),
        memory: Some(Memory {
            used: 4 << 30,
            total: 16 << 30,
        }),
        gpu: Some(Gpu {
            busy: Some(busy),
            memory: None,
            count: 1,
        }),
        ..Sample::default()
    }
}

/// Each device row carries its own host's GPU share while the sidebar shows
/// the GPU, whether or not CPU and memory show, and none once the sidebar's
/// GPU setting is off, whatever the status bar's says.
#[gpui::test]
fn each_device_carries_its_own_gpu_while_the_sidebar_shows_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let target = ConnectTarget::Ssh {
            target: "me@100.84.59.116".into(),
            session: "default".into(),
        };
        let remote = Host::load_of(&target).unwrap();
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            target,
            true,
        ));
        view.system_load.record(Host::Local, sample(10., 30.));
        view.system_load.record(remote, sample(20., 70.));
        view.config.show_system_load = false;
        view.config.show_gpu_load = true;
        view.config.show_sidebar_gpu_load = true;
        view
    });
    view.update(cx, |view, cx| {
        let loads = |view: &SheprdWindow, cx: &App| -> Vec<Value> {
            let state = view.hosted_state(cx);
            let devices = state.main["devices"].as_array().unwrap().clone();
            assert_eq!(devices.len(), 2, "{devices:?}");
            devices
                .iter()
                .map(|device| device["load"].clone())
                .collect()
        };

        // The GPU alone: CPU and memory are hidden.
        let gpu_only = loads(view, cx);
        assert_eq!(gpu_only[0]["gpu"]["busy"].as_f64(), Some(30.));
        assert_eq!(gpu_only[1]["gpu"]["busy"].as_f64(), Some(70.));
        assert!(gpu_only.iter().all(|load| load["cpu"].is_null()));

        view.config.show_system_load = true;
        let both = loads(view, cx);
        assert_eq!(both[1]["cpu"].as_f64(), Some(20.));
        assert_eq!(both[1]["gpu"]["busy"].as_f64(), Some(70.));

        // The sidebar's own setting hides it; the status bar's does not count.
        view.config.show_sidebar_gpu_load = false;
        let hidden = loads(view, cx);
        assert!(hidden.iter().all(|load| load["gpu"].is_null()));
        assert_eq!(hidden[0]["cpu"].as_f64(), Some(10.));
    });
}
