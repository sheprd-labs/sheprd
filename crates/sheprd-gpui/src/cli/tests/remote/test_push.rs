use super::*;

#[test]
fn remote_test_push_parses_with_json_and_window() {
    let invocation = parse(&["remote", "test-push", "--json", "--window", "2"]).unwrap();
    assert_eq!(
        invocation.command,
        ControlCommand::Remote(RemoteCommand::TestPush)
    );
    assert!(invocation.json);
    assert_eq!(invocation.window, Some(2));
    // It takes no device: every registered phone gets one.
    assert!(parse(&["remote", "test-push", "iPhone"]).is_err());
}
