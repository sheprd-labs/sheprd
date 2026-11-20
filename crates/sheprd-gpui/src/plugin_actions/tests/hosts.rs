//! Which endpoints' hosts are asked for actions at all: a development
//! session or a custom socket has no host a script reaches, and a window
//! built without a bridge, as headless tests build theirs, asks none.

use super::fake;
use crate::plugin_actions::{Error, Scope, Target};
use gpui::{Point, TestAppContext, VisualTestContext};
use sheprd_client::ConnectTarget;

#[derive(Debug, Clone, Copy)]
enum Case {
    Local,
    Development,
    CustomSocket,
    NoBridge,
}

fn shown(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.debug_bounds("plugin-action-hunk/review-tab").is_some()
}

#[gpui::test]
fn only_a_scriptable_host_is_asked_for_actions(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for case in [
        Case::Local,
        Case::Development,
        Case::CustomSocket,
        Case::NoBridge,
    ] {
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.dismiss_menu(window, cx);
                fake::install(v);
                fake::answer(fake::hunk_and_notes());
                match case {
                    Case::Local => {}
                    Case::Development => {
                        v.endpoints[0].connection.target = ConnectTarget::Session {
                            name: "default".into(),
                            development: true,
                        }
                    }
                    Case::CustomSocket => {
                        v.endpoints[0].connection.target =
                            ConnectTarget::Socket("/run/herdr/custom.sock".into())
                    }
                    Case::NoBridge => v.plugin_actions = Default::default(),
                }
                v.open_tab_menu("t1", None, Point::default(), window, cx);
            })
        });
        cx.run_until_parked();
        let asked = matches!(case, Case::Local);
        assert_eq!(fake::fetches(), usize::from(asked), "{case:?}");
        assert_eq!(shown(cx), asked, "{case:?}");

        let row = fake::hunk_and_notes().rows(Scope::Tab)[0].clone();
        let result = cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.run_plugin_action(&row, Target::Tab("t1".into()), None, window, cx)
            })
        });
        cx.run_until_parked();
        let run = fake::take_run();
        match case {
            Case::Local => {
                assert!(result.is_ok());
                assert_eq!(run, ["hunk/review-tab"]);
            }
            Case::Development => assert!(
                matches!(
                    result,
                    Err(crate::Error::PluginActions(Error::DevelopmentSession))
                ),
                "{result:?}"
            ),
            Case::CustomSocket | Case::NoBridge => assert!(
                matches!(
                    result,
                    Err(crate::Error::PluginActions(Error::UnsupportedHost))
                ),
                "{case:?}: {result:?}"
            ),
        }
        if !matches!(case, Case::Local) {
            assert!(run.is_empty(), "{case:?}");
        }
    }
}
