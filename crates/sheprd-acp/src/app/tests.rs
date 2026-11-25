use super::*;

#[test]
fn antigravity_auth_uses_explicit_method_then_configured_credentials() {
    assert_eq!(
        antigravity_auth_method_for(None, false, false),
        "oauth-personal"
    );
    assert_eq!(
        antigravity_auth_method_for(None, true, false),
        "gemini-api-key"
    );
    assert_eq!(
        antigravity_auth_method_for(None, false, true),
        "agent-platform"
    );
    assert_eq!(
        antigravity_auth_method_for(Some("oauth-business"), true, true),
        "oauth-business"
    );
}

#[test]
fn gemini_auth_uses_explicit_method_then_configured_credentials() {
    assert_eq!(
        gemini_auth_method_for(None, false, false, false),
        "oauth-personal"
    );
    assert_eq!(
        gemini_auth_method_for(None, true, false, false),
        "gemini-api-key"
    );
    assert_eq!(
        gemini_auth_method_for(None, false, true, false),
        "vertex-ai"
    );
    assert_eq!(gemini_auth_method_for(None, false, false, true), "gateway");
    assert_eq!(
        gemini_auth_method_for(Some("oauth-personal"), true, true, true),
        "oauth-personal"
    );
}

#[test]
fn choose_offers_antigravity() {
    let choices = choices_from(
        crate::agents::Agent::Claude,
        &[
            crate::agents::Agent::Antigravity,
            crate::agents::Agent::Gemini,
        ],
    );
    assert!(choices.contains(&crate::agents::Agent::Antigravity));
    assert!(choices.contains(&crate::agents::Agent::Claude));
    assert!(choices.contains(&crate::agents::Agent::Gemini));
}

#[test]
fn a_headless_chat_fails_unless_its_agent_ended_as_expected() {
    assert!(headless_result(true).is_ok());
    let error = headless_result(false).unwrap_err();
    assert!(matches!(
        error.downcast_ref::<crate::Error>(),
        Some(crate::Error::AgentEnded)
    ));
}
