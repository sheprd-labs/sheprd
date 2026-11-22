use super::*;
use crate::machine::Name;

fn name(text: &str) -> Name {
    Name::try_from(text.to_owned()).unwrap()
}

/// The menu lists the machines last read, under their own heading, then a
/// way to make one.
#[gpui::test]
fn the_menu_offers_each_machine_and_a_new_one(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = groups(&view, cx)[0];
    let before = cx.update(|_, cx| view.read(cx).group_actions(group, cx));
    assert!(before.contains(&Action::NewMachine));
    assert!(
        !before
            .iter()
            .any(|action| matches!(action, Action::Machine(_)))
    );

    cx.update(|_, cx| crate::machine::known::remember(vec![name("base"), name("work")], cx));
    let listed = cx.update(|_, cx| view.read(cx).group_actions(group, cx));
    let at = |action: &Action| listed.iter().position(|listed| listed == action).unwrap();
    assert!(at(&Action::Machine(name("base"))) < at(&Action::Machine(name("work"))));
    assert!(at(&Action::Machine(name("work"))) < at(&Action::NewMachine));
    assert_eq!(
        Action::Machine(name("work")).heading(),
        Some(crate::i18n::fl!("group-menu-machines"))
    );
    assert_eq!(
        Action::Machine(name("work")).selector(),
        "group-menu-Machine-work"
    );
}
