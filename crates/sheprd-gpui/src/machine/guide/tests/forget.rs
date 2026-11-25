#![allow(clippy::unwrap_used)]
use super::*;

/// A machine deleted and made again under its name starts its setup over,
/// whatever the old one's guide had seen.
#[gpui::test]
fn a_machine_made_again_starts_its_setup_over(cx: &mut gpui::TestAppContext) {
    let name = Name::try_from("guide-forget".to_owned()).unwrap();
    cx.update(|cx| {
        begin(&name, Opening::Shown, cx);
        signed_in(&name, cx);
        with(&name, |guide| guide.permissions = Some((true, true)));
        assert_eq!(shown(&name).map(|shown| shown.step), Some(Step::Save));
        forget(&name);
        assert_eq!(shown(&name), None);
        begin(&name, Opening::Shown, cx);
        assert_eq!(shown(&name).map(|shown| shown.step), Some(Step::SignIn));
        forget(&name);
    });
}
