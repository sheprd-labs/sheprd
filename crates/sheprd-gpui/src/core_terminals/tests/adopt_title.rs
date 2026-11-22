//! A phone's new workspace takes the name the person gave it there.

use crate::{
    browser::Store,
    sidebar::layout_tests::{SidebarFixture, fixture_window},
};
use gpui::{TestAppContext, prelude::*};
use sheprd_backend::{AgentStatus, RunState, TerminalId, TerminalInfo};

const FOLDER: &str = if cfg!(windows) {
    r"C:\code\api"
} else {
    "/code/api"
};

fn new_workspace(name: &str, title: Option<&str>) -> TerminalInfo {
    let mut labels = std::collections::BTreeMap::from([(
        sheprd_session::NEW_WORKSPACE_LABEL.to_owned(),
        FOLDER.to_owned(),
    )]);
    if let Some(title) = title {
        labels.insert(
            sheprd_session::NEW_WORKSPACE_TITLE_LABEL.to_owned(),
            title.to_owned(),
        );
    }
    TerminalInfo {
        agent: AgentStatus::Unknown,
        cwd: None,
        foreground: None,
        id: TerminalId::new(name).unwrap(),
        labels,
        name: None,
        reporter: None,
        state: RunState::Running,
        title: None,
    }
}

/// The name and whether the user gave it, of the workspace kept in
/// [`FOLDER`] after adopting `terminals` in turn.
fn kept_after(
    cx: &mut TestAppContext,
    terminals: impl IntoIterator<Item = TerminalInfo>,
) -> (String, bool) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|_, cx| {
        Store::update(cx, |_| {});
        view.update(cx, |view, cx| {
            for terminal in terminals {
                view.core_terminals.infos.insert(terminal);
                view.adopt_labelled_terminals(true, cx);
            }
        });
    });
    cx.update(|_, cx| {
        cx.global::<Store>()
            .kept_ids()
            .filter_map(|(scope, id)| {
                let scope = crate::browser::Scope::endpoint(scope);
                cx.global::<Store>().kept_workspace(&scope, id).cloned()
            })
            .find(|kept| kept.cwd == FOLDER)
            .map(|kept| (kept.label, kept.custom))
            .expect("a workspace kept in the folder")
    })
}

#[gpui::test]
fn a_titled_new_workspace_is_kept_under_that_title(cx: &mut TestAppContext) {
    let kept = kept_after(cx, [new_workspace("t5", Some("iOS vibration"))]);
    assert_eq!(kept, ("iOS vibration".to_owned(), true));
}

#[gpui::test]
fn an_untitled_new_workspace_is_named_for_its_folder(cx: &mut TestAppContext) {
    let kept = kept_after(cx, [new_workspace("t5", None)]);
    assert_eq!(kept, ("api".to_owned(), false));
}

#[gpui::test]
fn a_title_renames_the_workspace_kept_in_that_folder_already(cx: &mut TestAppContext) {
    let kept = kept_after(
        cx,
        [
            new_workspace("t5", None),
            new_workspace("t6", Some("Review")),
        ],
    );
    assert_eq!(kept, ("Review".to_owned(), true));
}

#[gpui::test]
fn no_title_keeps_the_name_of_the_workspace_it_joins(cx: &mut TestAppContext) {
    let kept = kept_after(
        cx,
        [
            new_workspace("t5", Some("Review")),
            new_workspace("t6", None),
        ],
    );
    assert_eq!(kept, ("Review".to_owned(), true));
}
