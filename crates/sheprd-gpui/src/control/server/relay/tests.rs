#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::fixture_window;
use gpui::TestAppContext;

fn mcp(client: Option<&str>) -> Via {
    Via::Mcp {
        client: client.map(str::to_owned),
    }
}

fn code(response: Option<Response>) -> Option<ErrorCode> {
    match response {
        Some(Response::Error { code, .. }) => Some(code),
        Some(other) => panic!("expected a refusal, got {other:?}"),
        None => None,
    }
}

#[gpui::test]
fn a_name_no_dialog_could_show_is_refused_whatever_the_switch(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let long = "x".repeat(crate::control::protocol::MAX_CLIENT_NAME + 1);
        assert_eq!(
            code(refusal(&mcp(Some(&long)), cx)),
            Some(ErrorCode::InvalidRequest)
        );
        assert_eq!(
            code(refusal(&mcp(Some("a\u{7}b")), cx)),
            Some(ErrorCode::InvalidRequest)
        );
    });
}

#[cfg(feature = "mcp")]
#[gpui::test]
fn mcp_requests_are_refused_without_a_window_to_read_the_switch_from(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(code(refusal(&mcp(None), cx)), Some(ErrorCode::NoWindow));
    });
}

#[cfg(feature = "mcp")]
#[gpui::test]
fn mcp_requests_follow_the_switch(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    assert!(
        view.read_with(cx, |view, _| !view.config.features.mcp),
        "off by default"
    );
    // The app's own context: a window context would hold the window read.
    cx.cx.update(|cx| {
        assert_eq!(
            code(refusal(&mcp(Some("Cursor")), cx)),
            Some(ErrorCode::Unsupported)
        );
    });
    view.update(cx, |view, _| view.config.features.mcp = true);
    cx.cx.update(|cx| {
        assert_eq!(code(refusal(&mcp(Some("Cursor")), cx)), None);
        assert_eq!(code(refusal(&mcp(None), cx)), None);
    });
}

#[cfg(not(feature = "mcp"))]
#[gpui::test]
fn a_build_without_mcp_refuses_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(code(refusal(&mcp(None), cx)), Some(ErrorCode::Unsupported));
    });
}

#[gpui::test]
fn a_dialog_an_mcp_client_opened_names_it_and_a_direct_one_does_not(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let flash = |view: &gpui::Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.flash.as_ref().map(|(flash, _)| flash.text.to_string())
        })
    };
    view.update(cx, |view, cx| announce(view, &Caller::default(), cx));
    assert_eq!(flash(&view, cx), None);
    let relayed = Caller {
        via: Some(mcp(Some("Cursor"))),
        ..Caller::default()
    };
    view.update(cx, |view, cx| announce(view, &relayed, cx));
    let text = flash(&view, cx).unwrap();
    assert!(text.contains("Cursor") && text.contains("MCP"), "{text}");
    let nameless = Caller {
        via: Some(mcp(None)),
        ..Caller::default()
    };
    view.update(cx, |view, cx| announce(view, &nameless, cx));
    assert!(flash(&view, cx).unwrap().contains("An MCP client"));
}
