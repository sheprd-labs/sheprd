use super::*;

fn info(accessibility: bool, screen_recording: bool) -> MachineAgentInfo {
    MachineAgentInfo {
        version: 1,
        width: 2560,
        height: 1600,
        scale: 2,
        accessibility,
        screen_recording,
        capabilities: None,
    }
}

#[test]
fn the_report_names_the_screen_in_the_unit_clicks_take() {
    let text = describe("vm", &info(true, true));
    assert!(text.contains("answers in vm"), "{text}");
    assert!(text.contains("2560x1600 pixels (2x)"), "{text}");
}

#[test]
fn the_report_says_which_permission_is_missing() {
    let text = describe("vm", &info(true, false));
    assert!(
        text.contains("Accessibility (click, type, key): allowed"),
        "{text}"
    );
    assert!(
        text.contains("Screen Recording (screenshot): not allowed yet"),
        "{text}"
    );
}
