use crate::control::protocol::{
    ErrorCode, Response,
    app::{Reply, UiOp},
};
use crate::settings_window::{catalog, guidance, tests::*};

#[gpui::test]
fn guidance_navigates_updates_and_clears_without_changing_settings(cx: &mut TestAppContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        let source = main.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(source.clone(), cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        let guide = main
            .read_with(cx, |view, _| view.agent_guidance.clone())
            .unwrap();
        let response = guidance::apply(source.clone(), UiOp::Settings { page: None }, cx);
        let Response::App {
            reply: Reply::SettingsTargets { pages, rows },
        } = response
        else {
            panic!("{response:?}");
        };
        for page in ["sidebar", "fleet", "tabs", "text-shortcuts"] {
            assert!(pages.contains(&page.into()));
        }
        assert!(pages.iter().all(|page| !page.starts_with("settings-")));
        assert!(
            rows.iter()
                .any(|row| row.id == "sidebar-indent" && row.page == "sidebar")
        );
        assert_eq!(
            pages.iter().collect::<std::collections::HashSet<_>>().len(),
            pages.len()
        );
        let op = UiOp::Guide {
            text: Some("This controls indentation".into()),
            highlight: Some("sidebar-indent".into()),
        };
        assert!(matches!(
            guidance::apply(source.clone(), op, cx),
            Response::Done { .. }
        ));
        settings
            .read_with(cx, |view, cx| {
                assert_eq!(view.section, Section::Sidebar);
                assert!(view.search_results.is_none());
                assert!(view.guidance_highlight(Some(&guide), cx).is_some());
            })
            .unwrap();
        assert_eq!(guide.read(cx).target, Some("sidebar-indent"));
        guidance::apply(
            source.clone(),
            UiOp::Guide {
                text: Some("Updated explanation".into()),
                highlight: None,
            },
            cx,
        );
        assert_eq!(guide.read(cx).text, "Updated explanation");
        assert_eq!(guide.read(cx).target, Some("sidebar-indent"));
        for op in [
            UiOp::Guide {
                text: Some(" ".into()),
                highlight: None,
            },
            UiOp::Guide {
                text: Some("x".repeat(501)),
                highlight: None,
            },
            UiOp::Guide {
                text: None,
                highlight: Some("unknown".into()),
            },
            UiOp::Settings {
                page: Some("unknown".into()),
            },
        ] {
            assert!(matches!(
                guidance::apply(source.clone(), op, cx),
                Response::Error { .. }
            ));
            assert_eq!(guide.read(cx).text, "Updated explanation");
        }
        let target = catalog::CATALOG
            .iter()
            .find(|row| row.id == "theme-side")
            .unwrap();
        if !settings
            .read_with(cx, |view, _| view.available(target.availability))
            .unwrap()
        {
            assert!(matches!(
                guidance::apply(
                    source.clone(),
                    UiOp::Guide {
                        text: None,
                        highlight: Some(target.id.into())
                    },
                    cx
                ),
                Response::Error {
                    code: ErrorCode::Unsupported,
                    ..
                }
            ));
        }
        guidance::apply(
            source.clone(),
            UiOp::Settings {
                page: Some("fonts".into()),
            },
            cx,
        );
        settings
            .read_with(cx, |view, cx| {
                assert_eq!(view.section, Section::Fonts);
                assert!(view.guidance_highlight(Some(&guide), cx).is_none());
            })
            .unwrap();
        guidance::apply(
            source,
            UiOp::Guide {
                text: None,
                highlight: None,
            },
            cx,
        );
        assert!(guide.read(cx).target.is_none());
    });
}
