use super::*;

/// The palette opens or makes a virtual Mac only where one can run.
#[gpui::test]
fn virtual_macs_are_offered_on_macos_alone(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::All, window, cx);
            let entries = &view.menu.palette.as_ref().unwrap().entries;
            for command in [Command::OpenMachine, Command::NewMachine] {
                let offered = entries.iter().any(
                    |entry| matches!(entry.action, Action::Native(listed) if listed == command),
                );
                assert_eq!(offered, cfg!(target_os = "macos"), "{command:?}");
            }
            view.dismiss_menu(window, cx);
        })
    });
}
