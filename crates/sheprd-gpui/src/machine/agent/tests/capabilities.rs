use super::*;
use sheprd_control_protocol::protocol::tabs::AgentCapabilities;

#[test]
fn legacy_macos_hello_derives_common_capabilities() {
    let body = br#"{"ok":true,"version":1,"width":2560,"height":1600,"scale":2,
        "accessibility":false,"screen_recording":true}"#;
    let hello: Hello = parse(body).unwrap();
    assert_eq!(hello.capabilities, None);
    assert_eq!(
        hello.effective_capabilities(),
        AgentCapabilities {
            exec: true,
            screenshot: true,
            pointer: false,
            keyboard: false,
            text: false,
            desktop_ready: true,
        }
    );
}

#[test]
fn linux_hello_parses_explicit_capabilities_without_macos_permission_flags() {
    let body = br#"{"ok":true,"version":1,"width":1280,"height":800,"scale":1,
        "capabilities":{"exec":true,"screenshot":true,"pointer":true,"keyboard":true,"text":true,"desktop_ready":true}}"#;
    let hello: Hello = parse(body).unwrap();
    assert_eq!(
        hello.effective_capabilities(),
        AgentCapabilities {
            exec: true,
            screenshot: true,
            pointer: true,
            keyboard: true,
            text: true,
            desktop_ready: true,
        }
    );
}

#[test]
fn linux_not_ready_and_unsupported_refusals_parse_distinctly() {
    let not_ready = br#"{"ok":false,"error":"not_ready","message":"Wayland session starting"}"#;
    let err = parse::<Done>(not_ready).unwrap_err();
    assert!(matches!(
        err,
        Error::Refused {
            refusal: Refusal::NotReady,
            ref message,
        } if message == "Wayland session starting"
    ));

    let unsupported = br#"{"ok":false,"error":"unsupported","message":"grim not installed"}"#;
    let err = parse::<Done>(unsupported).unwrap_err();
    assert!(matches!(
        err,
        Error::Refused {
            refusal: Refusal::Unsupported,
            ref message,
        } if message == "grim not installed"
    ));
}

#[test]
fn png_size_rejects_zero_and_oversized_dimensions() {
    let make = |w: u32, h: u32| {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&w.to_be_bytes());
        png.extend_from_slice(&h.to_be_bytes());
        png
    };
    assert_eq!(png_size(&make(0, 800)), None);
    assert_eq!(png_size(&make(1280, 0)), None);
    assert_eq!(png_size(&make(20_000, 800)), None);
    assert_eq!(png_size(&make(1280, 800)), Some((1280, 800)));
}
