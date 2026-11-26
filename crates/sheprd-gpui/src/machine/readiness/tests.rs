use super::*;
use sheprd_control_protocol::protocol::tabs::AgentCapabilities;

fn hello(desktop_ready: bool) -> agent::Hello {
    agent::Hello {
        version: 1,
        width: 1280,
        height: 800,
        scale: 1.0,
        accessibility: desktop_ready,
        screen_recording: desktop_ready,
        capabilities: Some(AgentCapabilities {
            exec: true,
            screenshot: desktop_ready,
            pointer: desktop_ready,
            keyboard: desktop_ready,
            text: desktop_ready,
            desktop_ready,
        }),
    }
}

#[test]
fn transitions_through_booting_configuring_and_ready() {
    assert_eq!(
        evaluate(&Status::Stopped, &Readiness::Booting, None),
        Readiness::NotRunning
    );
    assert_eq!(
        evaluate(&Status::Starting, &Readiness::NotRunning, None),
        Readiness::Booting
    );
    assert_eq!(
        evaluate(&Status::Running, &Readiness::Booting, None),
        Readiness::Booting
    );
    assert_eq!(
        evaluate(&Status::Running, &Readiness::Booting, Some(&hello(false))),
        Readiness::Configuring
    );
    assert_eq!(
        evaluate(
            &Status::Running,
            &Readiness::Configuring,
            Some(&hello(true))
        ),
        Readiness::Ready
    );
    // Transient unreachable poll after ready does not flicker back to Booting.
    assert_eq!(
        evaluate(&Status::Running, &Readiness::Ready, None),
        Readiness::Ready
    );
    assert_eq!(
        evaluate(&Status::Failed("boom".into()), &Readiness::Ready, None),
        Readiness::Failed("boom".into())
    );
}
