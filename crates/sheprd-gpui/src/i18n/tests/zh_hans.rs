use super::*;

#[test]
fn simplified_chinese_is_available_and_formats_messages() {
    let locale: Locale = "zh-Hans".parse().unwrap();
    assert!(available().contains(&locale));
    assert_eq!(endonym(&locale), "简体中文");
    show(&locale);

    assert_eq!(fl!("settings-title"), "设置");
    assert_eq!(
        fl!("chat-preparing-pictures", count = 3),
        "正在准备 \u{2068}3\u{2069} 张图片…"
    );
    assert_eq!(fl!("sidebar-activity-security"), "安全");
    // The security page itself has no Chinese catalog yet.
    assert_eq!(fl!("security-title"), english("security-title", &[]));
}

#[test]
fn system_simplified_chinese_preferences_resolve_to_the_catalog() {
    let chinese: Locale = "zh-Hans".parse().unwrap();
    for tag in ["zh-Hans", "zh-CN", "zh-SG"] {
        let requested = [tag.parse().unwrap()];
        assert_eq!(resolve(&Language::System, &requested), chinese);
        assert_eq!(
            resolve(&Language::Locale(requested[0].clone().into()), &requested),
            Locale::FALLBACK
        );
    }
}
