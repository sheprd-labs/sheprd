use super::*;
use crate::config::preferences::Preference;
use std::collections::HashSet;

#[test]
fn every_listed_feature_reads_its_own_key() -> anyhow::Result<()> {
    let keys: HashSet<_> = Feature::ALL.iter().map(|feature| feature.key()).collect();
    assert_eq!(keys.len(), Feature::ALL.len());
    for &feature in Feature::ALL {
        let none: &[&str] = &[];
        assert_eq!(crate::i18n::takes(feature.message()), Some(none));
        assert_eq!(
            crate::i18n::takes(feature.description_message()),
            Some(none)
        );
        let defaults = Features::default();
        let value = !defaults.enabled(feature);
        let config = Config::parse(&format!("[features]\n{} = {value}", feature.key()))?;
        for &other in Feature::ALL {
            let expected = if other == feature {
                value
            } else {
                defaults.enabled(other)
            };
            assert_eq!(config.features.enabled(other), expected, "{other:?}");
        }
    }
    Ok(())
}

#[test]
fn feature_preferences_write_under_the_features_table() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    for &feature in Feature::ALL {
        for value in [true, false] {
            Config::save_preference_path(Preference::Feature(feature, value), &path)?;
            let config = Config::parse(&fs::read_to_string(&path)?)?;
            assert_eq!(config.features.enabled(feature), value, "{feature:?}");
        }
    }
    Ok(())
}

#[test]
fn the_moonframe_window_requires_opt_in() -> anyhow::Result<()> {
    assert!(!Config::parse("")?.features.moonframe_window);
    assert!(!Config::parse("[features]")?.features.moonframe_window);
    let config = Config::parse("[features]\nmoonframe_window = true")?;
    assert!(config.features.moonframe_window);
    assert!(!config.features.moonframe_tools);
    Ok(())
}

/// Each Cargo feature of this crate that adds something users see, the
/// `[features]` switch that turns it off at runtime, and whether this build
/// has it.
const SWITCHED: &[(&str, &str, bool)] = &[
    ("azure", "cloud_azure", cfg!(feature = "azure")),
    ("boat", "cloud_boat", cfg!(feature = "boat")),
    ("boxd", "cloud_boxd", cfg!(feature = "boxd")),
    ("cloud", "cloud_providers", cfg!(feature = "cloud")),
    ("coder", "cloud_coder", cfg!(feature = "coder")),
    ("container", "cloud_container", cfg!(feature = "container")),
    ("daytona", "cloud_daytona", cfg!(feature = "daytona")),
    ("mcp", "mcp", cfg!(feature = "mcp")),
    ("moonframe", "moonframe_window", cfg!(feature = "moonframe")),
    (
        "opensandbox",
        "cloud_opensandbox",
        cfg!(feature = "opensandbox"),
    ),
    ("railway", "cloud_railway", cfg!(feature = "railway")),
    ("sprites", "cloud_sprites", cfg!(feature = "sprites")),
];

/// Cargo features that only shape a build, so a runtime switch would change
/// nothing, and why.
const BUILD_ONLY: &[(&str, &str)] = &[
    ("integration-test", "test code and fixtures"),
    (
        "mas",
        "the Mac App Store build: store review checks what is compiled in",
    ),
    ("mockup", "a development window, never in a release build"),
    (
        "moonframe-capture",
        "frame captures for checks, never in a release build",
    ),
    (
        "moonframe-studio",
        "reloads the layout from a checkout, never in a release build",
    ),
    (
        "tmux",
        "an opt-in build whose backend runs only once chosen under Settings > Tabs",
    ),
];

/// A new Cargo feature that users would see needs a `[features]` switch, so
/// it can ship compiled in and still be turned off later; one that only
/// shapes the build says so in `BUILD_ONLY`.
#[test]
fn every_cargo_feature_has_a_switch_or_is_build_only() -> anyhow::Result<()> {
    let manifest: toml::Table = toml::from_str(include_str!("../../../Cargo.toml"))?;
    let Some(cargo) = manifest.get("features").and_then(toml::Value::as_table) else {
        anyhow::bail!("Cargo.toml has no [features] table");
    };
    let listed: HashSet<&str> = SWITCHED
        .iter()
        .map(|(name, ..)| *name)
        .chain(BUILD_ONLY.iter().map(|(name, _)| *name))
        .collect();
    for name in cargo.keys().filter(|name| *name != "default") {
        assert!(
            listed.contains(name.as_str()),
            "Cargo feature `{name}` needs a `[features]` switch (SWITCHED) or a reason in BUILD_ONLY"
        );
    }
    for name in &listed {
        assert!(
            cargo.contains_key(*name),
            "`{name}` is no longer a Cargo feature"
        );
    }
    let keys: HashSet<_> = Feature::ALL.iter().map(|feature| feature.key()).collect();
    for &(name, key, built) in SWITCHED {
        assert!(
            !built || keys.contains(key),
            "Cargo feature `{name}` is built but `[features] {key}` does not exist"
        );
    }
    Ok(())
}
