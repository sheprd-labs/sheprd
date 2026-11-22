use super::*;

fn select(id: &str, category: OptionCategory, current: &str) -> ConfigOption {
    ConfigOption {
        category,
        description: None,
        id: id.into(),
        kind: OptionKind::Select {
            choices: vec![Choice {
                description: None,
                group: None,
                id: current.into(),
                name: current.into(),
            }],
            current: current.into(),
        },
        name: id.into(),
    }
}

fn chat() -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.auto_accept = true;
    transcript.modes = Some(Modes {
        available: Vec::new(),
        current: "plan".into(),
    });
    transcript.options = vec![
        select("effort", OptionCategory::ThoughtLevel, "high"),
        select("other", OptionCategory::Other, "x"),
        select("model", OptionCategory::Model, "opus"),
        ConfigOption {
            category: OptionCategory::ModelConfig,
            description: None,
            id: "fast".into(),
            kind: OptionKind::Toggle { current: true },
            name: "Fast".into(),
        },
    ];
    transcript
}

#[test]
fn settings_carry_the_model_first_and_leave_unknown_kinds_out() {
    let settings = Settings::of(Agent::Claude, &chat());
    assert!(settings.is_for(Agent::Claude));
    assert!(!settings.is_for(Agent::Codex));
    assert!(settings.auto_accept);
    assert_eq!(settings.mode.as_deref(), Some("plan"));
    let ids: Vec<&str> = settings.options.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["model", "fast", "effort"]);
    assert_eq!(
        settings.options[1].value,
        OptionValue::Toggle { value: true }
    );
}

#[test]
fn settings_round_trip_through_the_variable() {
    let settings = Settings::of(Agent::Claude, &chat());
    let text = settings.encode().unwrap();
    assert_eq!(Settings::decode(&text).unwrap(), settings);
}

#[test]
fn settings_past_the_limit_are_refused_both_ways() {
    let mut settings = Settings::of(Agent::Claude, &chat());
    settings.mode = Some("m".repeat(MAX_SETTINGS));
    assert!(matches!(
        settings.encode(),
        Err(Error::SettingsLimit { .. })
    ));
    let long = format!("{{\"agent\":\"{}\"}}", "a".repeat(MAX_SETTINGS));
    assert!(matches!(
        Settings::decode(&long),
        Err(Error::SettingsLimit { .. })
    ));
    assert!(matches!(Settings::decode("not json"), Err(Error::Json(_))));
}
