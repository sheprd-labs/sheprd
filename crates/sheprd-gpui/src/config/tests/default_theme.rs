use super::*;

/// WCAG 2 contrast ratio between two packed RGB colors.
fn contrast(a: u32, b: u32) -> f64 {
    let luminance = |color: u32| {
        let channel = |shift: u32| {
            let value = f64::from((color >> shift) & 255) / 255.;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn every_default_text_color_reads_on_its_background() {
    let theme = Theme::default();
    assert!(contrast(theme.foreground, theme.background) >= 7.);
    // Red through white and their bright forms carry text: at least WCAG AA.
    for index in (1..8).chain(9..16) {
        let ratio = contrast(theme.palette[index], theme.background);
        assert!(
            ratio >= 4.5,
            "color {index} is {ratio:.1}:1 on the background"
        );
    }
    // Bright black is for dimmed text such as timings and comments: quieter,
    // but still legible.
    let dim = contrast(theme.palette[8], theme.background);
    assert!(dim >= 3.5, "bright black is {dim:.1}:1 on the background");
}

#[test]
fn the_default_accent_stands_out_from_the_chrome() {
    let theme = Theme::default();
    assert_eq!(theme.primary(), theme.palette[5]);
    assert!(contrast(theme.primary(), theme.surface) >= 4.5);
}

#[test]
fn each_bright_default_color_is_lighter_than_its_normal_one() {
    let palette = Theme::default().palette;
    let background = Theme::default().background;
    for index in 1..8 {
        assert!(
            contrast(palette[index + 8], background) > contrast(palette[index], background),
            "bright color {} is no brighter than color {index}",
            index + 8
        );
    }
}
