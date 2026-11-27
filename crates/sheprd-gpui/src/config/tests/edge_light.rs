use super::*;
use crate::config::preferences::Preference;

#[test]
fn edge_light_defaults_and_boundaries() -> anyhow::Result<()> {
    let defaults = EdgeLight {
        color: None,
        width: 5,
        trail: 16,
    };
    assert_eq!(Config::parse("")?.edge_light, defaults);
    assert_eq!(Config::parse(DEFAULT_CONFIG)?.edge_light, defaults);
    for (width, trail) in [(1, 5), (12, 50)] {
        let config = Config::parse(&format!(
            "[edge_light]\nwidth = {width}\ntrail = {trail}\ncolor = '#aB15C0'"
        ))?;
        assert!(config.unknown_keys.is_empty());
        assert_eq!(
            config.edge_light,
            EdgeLight {
                color: Some(0xab15c0),
                width,
                trail
            }
        );
    }
    Ok(())
}

#[test]
fn edge_light_cli_rejects_invalid_overrides_without_writing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let local = dir.path().join("local.toml");
    let daemon = dir.path().join("missing.toml");
    let initial = "[edge_light]\ncolor = 'auto'\n";
    fs::write(&local, initial)?;
    for (key, value) in [
        ("edge_light.width", "0"),
        ("edge_light.width", "13"),
        ("edge_light.trail", "4"),
        ("edge_light.trail", "51"),
        ("edge_light.color", "#12xx33"),
        ("edge_light.color", "#12345678"),
        ("edge_light.future", "1"),
    ] {
        assert!(
            Config::edit_key_at(&local, &daemon, key, Some(parse_value(value))).is_err(),
            "{key} = {value}"
        );
        assert_eq!(fs::read_to_string(&local)?, initial);
    }
    Ok(())
}

#[test]
fn edge_light_preferences_and_cli_round_trip_individual_keys() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config-gpui.toml");
    let local = path.with_extension("local.toml");
    let daemon = dir.path().join("missing.toml");
    fs::write(
        &local,
        "[edge_light]\nwidth = 2 # keep\nfuture = 99\n[other]\nkeep = true\n",
    )?;
    for edit in [
        Preference::EdgeLightWidth(9),
        Preference::EdgeLightTrail(45),
        Preference::EdgeLightColor(Some(0x010203)),
    ] {
        Config::save_preference_path(edit, &local)?;
    }
    let saved = fs::read_to_string(&local)?;
    assert!(saved.contains("width = 9 # keep"));
    assert!(saved.contains("future = 99"));
    assert!(saved.contains("[other]\nkeep = true"));
    assert_eq!(
        Config::check_files_at(&path, &daemon)?.edge_light,
        EdgeLight {
            color: Some(0x010203),
            width: 9,
            trail: 45
        }
    );
    Config::save_preference_path(Preference::EdgeLightColor(None), &local)?;
    assert_eq!(
        Config::effective_value_at(&path, "edge_light.color")?,
        Some((toml::Value::String("auto".into()), Layer::Default))
    );
    for (key, value) in [
        ("edge_light.width", "2"),
        ("edge_light.trail", "8"),
        ("edge_light.color", "#32de87"),
    ] {
        Config::edit_key_at(&local, &daemon, key, Some(parse_value(value)))?;
    }
    assert_eq!(
        Config::check_files_at(&path, &daemon)?.edge_light,
        EdgeLight {
            color: Some(0x32de87),
            width: 2,
            trail: 8
        }
    );
    Ok(())
}
