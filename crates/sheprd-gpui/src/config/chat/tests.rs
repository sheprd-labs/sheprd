#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn reading_width_defaults_to_800_and_accepts_full_or_bounded_columns() {
    use crate::config::Config;
    assert_eq!(ChatConfig::default().max_width, DEFAULT_MAX_WIDTH);
    assert_eq!(Config::parse("").unwrap().chat.max_width, DEFAULT_MAX_WIDTH);
    assert_eq!(
        Config::parse("[chat]\nglide = true")
            .unwrap()
            .chat
            .max_width,
        DEFAULT_MAX_WIDTH
    );
    for width in [
        0,
        *MAX_WIDTH_RANGE.start(),
        800,
        1_200,
        *MAX_WIDTH_RANGE.end(),
    ] {
        let config = Config::parse(&format!("[chat]\nmax_width = {width}\n")).unwrap();
        assert_eq!(config.chat.max_width, width);
        assert!(config.unknown_keys.is_empty());
    }
    for value in [
        "-1", "1", "319", "4001", "65535", "65536", "800.5", "nan", "'wide'",
    ] {
        assert!(
            Config::parse(&format!("[chat]\nmax_width = {value}\n")).is_err(),
            "{value}"
        );
    }
}

#[test]
fn width_preferences_preserve_other_chat_settings_and_refuse_invalid_saves() {
    use crate::config::{Config, preferences::Preference};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("local.toml");
    std::fs::write(
        &path,
        "# keep\n[chat]\nlayout = 'imessage'\nglide = true\n[chat.env]\nGH_HOST = 'ghe.example'\n",
    )
    .unwrap();
    for width in [1_200, 0, DEFAULT_MAX_WIDTH] {
        Config::save_preference_path(Preference::ChatMaxWidth(width), &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&text).unwrap();
        assert_eq!(config.chat.max_width, width);
        assert_eq!(config.chat.layout, ChatLayoutMode::IMessage);
        assert!(config.chat.glide);
        assert_eq!(config.chat.env.get("GH_HOST").unwrap(), "ghe.example");
        assert!(text.starts_with("# keep"));
    }
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(Config::save_preference_path(Preference::ChatMaxWidth(1), &path).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), saved);
}

#[test]
fn cli_width_edits_validate_before_writing_and_unset_restores_default() {
    use crate::config::{Config, parse_value};
    let directory = tempfile::tempdir().unwrap();
    let local = directory.path().join("local.toml");
    let daemon = directory.path().join("daemon.toml");
    std::fs::write(&local, "# keep\n[chat]\nlayout = 'cards'\n").unwrap();
    for value in ["1200", "0"] {
        assert!(
            Config::edit_key_at(&local, &daemon, "chat.max_width", Some(parse_value(value)))
                .unwrap()
        );
        let config = Config::parse(&std::fs::read_to_string(&local).unwrap()).unwrap();
        assert_eq!(config.chat.max_width.to_string(), value);
        assert_eq!(config.chat.layout, ChatLayoutMode::Cards);
    }
    let saved = std::fs::read_to_string(&local).unwrap();
    assert!(
        Config::edit_key_at(&local, &daemon, "chat.max_width", Some(parse_value("319"))).is_err()
    );
    assert_eq!(std::fs::read_to_string(&local).unwrap(), saved);
    assert!(Config::edit_key_at(&local, &daemon, "chat.max_width", None).unwrap());
    assert_eq!(
        Config::parse(&std::fs::read_to_string(local).unwrap())
            .unwrap()
            .chat
            .max_width,
        DEFAULT_MAX_WIDTH
    );
}

#[test]
fn rendering_existing_agents_is_explicitly_opted_in() {
    assert!(
        !crate::config::Config::parse("")
            .unwrap()
            .chat
            .render_existing
    );
    assert!(
        crate::config::Config::parse("[chat]\nrender_existing = true\n")
            .unwrap()
            .chat
            .render_existing
    );
}

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn mirrors_accept_only_bounded_explicit_endpoint_ids() {
    let config = crate::config::Config::parse(
        "[[chat.mirrors]]\npane = 'w1:p1'\nendpoint = '0123456789abcdef0123456789abcdef'\n",
    )
    .unwrap();
    assert_eq!(config.chat.mirrors.len(), 1);
    assert_eq!(config.chat.mirrors[0].host, None);
    assert!(
        crate::config::Config::parse(
            "[[chat.mirrors]]\npane = 'w1:p1'\nendpoint = '/tmp/arbitrary.sock'\n"
        )
        .is_err()
    );
    let mut chat = config.chat;
    chat.mirrors = vec![chat.mirrors[0].clone(); 17];
    assert!(matches!(
        chat.check(),
        Err(crate::chat::Error::MirrorConfig)
    ));
}

#[test]
fn the_field_reads_and_writes_name_value_pairs() {
    let parsed =
        parse_env(r#" GH_HOST=mycompany.ghe.com  NOTE="two words" PATHS="a\"b" EMPTY="" "#);
    assert_eq!(
        parsed,
        Some(env(&[
            ("EMPTY", ""),
            ("GH_HOST", "mycompany.ghe.com"),
            ("NOTE", "two words"),
            ("PATHS", "a\"b"),
        ]))
    );
    let text = env_text(&parsed.unwrap());
    assert_eq!(
        text,
        r#"EMPTY="" GH_HOST=mycompany.ghe.com NOTE="two words" PATHS="a\"b""#
    );
    assert_eq!(
        parse_env(&text).map(|env| env.len()),
        Some(4),
        "round trips"
    );
    assert_eq!(parse_env(""), Some(BTreeMap::new()));
}

#[test]
fn malformed_or_unsafe_pairs_are_refused() {
    for text in [
        "GH_HOST",
        "=value",
        "1ABC=x",
        "MY-VAR=x",
        "A=\"unclosed",
        "A=\"x\"y",
        "HERDR_ACP=/tmp/evil",
    ] {
        assert_eq!(parse_env(text), None, "{text}");
    }
    assert!(check_env(&env(&[("A", "x\0y")])).is_err());
    let many: BTreeMap<String, String> = (0..=MAX_ENV)
        .map(|i| (format!("V{i}"), String::new()))
        .collect();
    assert!(matches!(
        check_env(&many),
        Err(crate::chat::Error::EnvironmentSize)
    ));
    let large = env(&[("BIG", &"x".repeat(MAX_ENV_BYTES))]);
    assert!(matches!(
        check_env(&large),
        Err(crate::chat::Error::EnvironmentSize)
    ));
}

#[test]
fn the_layout_is_read_and_saved_by_name() {
    let config = crate::config::Config::parse("[chat]\nlayout = \"cards\"\n").unwrap();
    assert_eq!(config.chat.layout, ChatLayoutMode::Cards);
    assert!(crate::config::Config::parse("[chat]\nlayout = \"stack\"\n").is_err());
    // A retired layout a config may still name opens in its closest one.
    assert_eq!(
        crate::config::Config::parse("[chat]\nlayout = \"pill\"\n")
            .unwrap()
            .chat
            .layout,
        ChatLayoutMode::Bubbles
    );
    assert_eq!(
        crate::config::Config::parse("").unwrap().chat.layout,
        ChatLayoutMode::Timeline
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    std::fs::write(&path, "[chat.env]\nGH_HOST = \"ghe.example\"\n").unwrap();
    for &mode in ChatLayoutMode::ALL {
        crate::config::Config::save_preference_path(
            crate::config::preferences::Preference::ChatLayout(mode),
            &path,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let config = crate::config::Config::parse(&text).unwrap();
        assert_eq!(config.chat.layout, mode);
        assert_eq!(config.chat.env.get("GH_HOST").unwrap(), "ghe.example");
    }
}

#[test]
fn chats_jump_to_new_output_unless_glide_is_on() {
    assert!(!crate::config::Config::parse("").unwrap().chat.glide);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    std::fs::write(&path, "[chat]\nlayout = \"imessage\"\n").unwrap();
    for glide in [true, false] {
        crate::config::Config::save_preference_path(
            crate::config::preferences::Preference::ChatGlide(glide),
            &path,
        )
        .unwrap();
        let config =
            crate::config::Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config.chat.glide, glide);
        assert_eq!(config.chat.layout, ChatLayoutMode::IMessage);
    }
}

mod auto_accept;
mod code;
mod launcher;

#[test]
fn new_agents_stay_in_their_terminals_unless_auto_convert_is_on() {
    assert!(!crate::config::Config::parse("").unwrap().chat.auto_convert);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    std::fs::write(&path, "[chat]\nlayout = \"imessage\"\n").unwrap();
    for auto_convert in [true, false] {
        crate::config::Config::save_preference_path(
            crate::config::preferences::Preference::ChatAutoConvert(auto_convert),
            &path,
        )
        .unwrap();
        let config =
            crate::config::Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config.chat.auto_convert, auto_convert);
        assert_eq!(config.chat.layout, ChatLayoutMode::IMessage);
    }
}

#[test]
fn message_effects_default_on_and_the_switch_persists() {
    use crate::config::{Config, preferences::Preference};
    assert!(Config::parse("").unwrap().chat.message_effects);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    for enabled in [false, true] {
        Config::save_preference_path(Preference::ChatMessageEffects(enabled), &path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert_eq!(Config::parse(&saved).unwrap().chat.message_effects, enabled);
    }
    let daemon = dir.path().join("daemon.toml");
    let key = "chat.message_effects";
    Config::edit_key_at(&path, &daemon, key, Some(false.into())).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(!Config::parse(&saved).unwrap().chat.message_effects);
    Config::edit_key_at(&path, &daemon, key, None).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(Config::parse(&saved).unwrap().chat.message_effects);
}

#[test]
fn long_text_rules_default_to_the_limits_chats_used_before_they_were_settings() {
    let chat = ChatConfig::default();
    // A paste attached past 10 lines or from 800 characters; a message
    // showed as a file from 12 lines or 2048 characters.
    let paste = chat.long_paste().unwrap();
    assert!(!paste.matches(&"a\n".repeat(10)));
    assert!(paste.matches(&"a\n".repeat(11)));
    assert!(!paste.matches(&"x".repeat(799)));
    assert!(paste.matches(&"x".repeat(800)));
    let message = chat.long_message().unwrap();
    assert!(!message.matches(&"line\r\n".repeat(11)));
    assert!(message.matches(&"line\r\n".repeat(12)));
    assert!(!message.matches(&"界".repeat(2047)));
    assert!(message.matches(&"界".repeat(2048)));
}

#[test]
fn a_long_text_rule_meets_either_limit_counting_characters_and_lines() {
    let rule = LongText { chars: 5, lines: 3 };
    assert!(!rule.matches(""));
    assert!(!rule.matches("abcd"));
    assert!(rule.matches("abcde"));
    // Characters, not bytes.
    assert!(!rule.matches("éééé"));
    // A trailing newline ends a line rather than starting one.
    assert!(!rule.matches("a\nb\n"));
    assert!(rule.matches("a\nb\nc"));
    let one = LongText { chars: 1, lines: 1 };
    assert!(!one.matches(""));
    assert!(one.matches("a"));
}

#[test]
fn long_text_keys_parse_turn_off_and_refuse_limits_out_of_range() {
    use crate::config::Config;
    let config = Config::parse(
        "[chat]\nlong_paste_as_file = false\nlong_paste_chars = 5\nlong_paste_lines = 2\n\
         long_message_chars = 99999\nlong_message_lines = 1\n",
    )
    .unwrap();
    assert!(config.unknown_keys.is_empty());
    assert_eq!(config.chat.long_paste(), None);
    assert_eq!(config.chat.long_paste_chars, 5);
    assert_eq!(config.chat.long_paste_lines, 2);
    assert_eq!(
        config.chat.long_message(),
        Some(LongText {
            chars: 99_999,
            lines: 1
        })
    );
    for (key, value) in [
        ("long_paste_chars", "0"),
        ("long_paste_chars", "100000"),
        ("long_message_chars", "-1"),
        ("long_paste_lines", "0"),
        ("long_message_lines", "1000"),
        ("long_message_lines", "2.5"),
    ] {
        assert!(
            Config::parse(&format!("[chat]\n{key} = {value}\n")).is_err(),
            "{key} = {value}"
        );
    }
}

#[test]
fn long_text_preferences_save_their_keys_and_refuse_limits_out_of_range() {
    use crate::config::{Config, preferences::Preference};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("local.toml");
    std::fs::write(&path, "# keep\n[chat]\nglide = true\n").unwrap();
    for edit in [
        Preference::ChatLongPaste(false),
        Preference::ChatLongPasteChars(30),
        Preference::ChatLongPasteLines(4),
        Preference::ChatLongMessage(false),
        Preference::ChatLongMessageChars(40),
        Preference::ChatLongMessageLines(5),
    ] {
        Config::save_preference_path(edit, &path).unwrap();
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let chat = Config::parse(&text).unwrap().chat;
    assert!(text.starts_with("# keep"));
    assert!(chat.glide);
    assert!(!chat.long_paste_as_file && !chat.long_message_as_file);
    assert_eq!((chat.long_paste_chars, chat.long_paste_lines), (30, 4));
    assert_eq!((chat.long_message_chars, chat.long_message_lines), (40, 5));
    for edit in [
        Preference::ChatLongPasteChars(0),
        Preference::ChatLongPasteLines(1_000),
        Preference::ChatLongMessageChars(100_000),
        Preference::ChatLongMessageLines(0),
    ] {
        assert!(Config::save_preference_path(edit, &path).is_err());
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
}
