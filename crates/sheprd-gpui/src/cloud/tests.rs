use super::*;

#[test]
fn every_built_in_provider_is_on_by_default() {
    assert_eq!(enabled_in(&Features::default()), CloudProvider::ALL);
}

#[test]
fn the_cloud_providers_switch_turns_every_provider_off() {
    let features = Features {
        cloud_providers: false,
        ..Features::default()
    };
    assert!(enabled_in(&features).is_empty());
}

#[test]
fn a_provider_switch_turns_only_that_provider_off() -> anyhow::Result<()> {
    for &provider in CloudProvider::ALL {
        let features: Features = toml::from_str(&format!("{} = false", switch(provider).key()))?;
        let expected: Vec<_> = CloudProvider::ALL
            .iter()
            .copied()
            .filter(|&other| other != provider)
            .collect();
        assert_eq!(enabled_in(&features), expected, "{provider:?}");
    }
    Ok(())
}
