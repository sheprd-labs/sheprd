//! A kept workspace follows its first driving terminal across `cd`, even
//! when an empty split group has focus. User names remain user names.

use crate::{
    browser::Store,
    core_terminals::{
        Purpose,
        tests::kept_workspaces::{core_tab, window},
    },
};
use gpui::TestAppContext;
use sheprd_backend::{RunState, Spawn, TerminalId, TerminalInfo};
use sheprd_chat_protocol::Agent;
use sheprd_client::protocol::AgentStatus;
use std::path::{Path, PathBuf};

#[gpui::test]
fn live_folder_drives_metadata_empty_groups_and_chat_tabs(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, false);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let workspace = view.open_kept_workspace(None, Path::new("/old"), core_tab("t1"), window, cx).unwrap();
            let scope = crate::browser::scope(&view.endpoints[0]);
            let tab = cx.global::<Store>().in_workspace(&scope, &workspace).next().unwrap().id;
            let terminal = TerminalId::new("t1").unwrap();
            view.core_terminals.show_for_test(tab, crate::core_terminals::panes::Panes::single(terminal.clone()), false, vec![TerminalInfo {
                agent: AgentStatus::Unknown,
                cwd: Some(PathBuf::from("/work/repo/src/nested")),
                foreground: None,
                id: terminal,
                labels: Default::default(),
                name: None,
                reporter: None,
                state: RunState::Running,
                title: None,
            }]);
            view.core_terminals.seed_checkout("/work/repo/src/nested", Some(sheprd_vcs::git::Identity {
                branch: Some("feature".into()), key: "/work/repo/.git".into(), label: "repo".into(), linked: false, root: "/work/repo".into(),
            }));
            view.refresh_core_session(cx);
            let sessions = view.sessions(0);
            let current = sessions.workspace(&workspace).unwrap();
            assert_eq!(current.cwd, "/work/repo/src/nested");
            assert_eq!(current.label, "repo");
            assert_eq!(current.branch.as_deref(), Some("feature"));
            view.rename_kept_workspace(&workspace, "Chosen name", cx);
            assert_eq!(view.sessions(0).workspace(&workspace).unwrap().label, "Chosen name");

            view.split_active_group(window, cx);
            assert!(view.shown_core_pane(cx).is_none());
            assert!(matches!(view.group_shown(view.active_group().unwrap(), cx), crate::browser::Shown::Empty),
                "an empty Herdr 2 group must not expose Herdr 1's reconnect screen");
            let mut spawn = Spawn::default();
            view.give_core_context(&Purpose::Tab { focus: true, group: None, scope: scope.clone(), workspace: workspace.clone() }, &crate::core_terminals::Host::Local, &mut spawn, cx);
            assert_eq!(spawn.cwd.as_deref(), Some(Path::new("/work/repo/src/nested")));

            view.config.features.acp_chats = true;
            view.chats.program = crate::chat::Program::Found("/opt/sheprd-acp".into());
            view.start_core_chat_in_tab(Agent::Codex, tab, None, cx).unwrap();
            let (purpose, _, spawn, _) = view.core_terminals.wanted.back().unwrap();
            assert!(matches!(purpose, Purpose::Tab { scope: actual, workspace: id, .. } if actual == &scope && id == &workspace));
            assert_eq!(spawn.cwd.as_deref(), Some(Path::new("/work/repo/src/nested")));
            view.close_browser_tab(tab, window, cx);
            view.forget_core_tabs_for_test();
            view.refresh_core_session(cx);
            let sessions = view.sessions(0);
            let current = sessions.workspace(&workspace).unwrap();
            assert_eq!(current.cwd, "/work/repo/src/nested");
            assert_eq!(current.label, "Chosen name");
            assert_eq!(current.branch.as_deref(), Some("feature"));
            let saved = cx.global::<Store>().kept_workspace(&scope, &workspace).unwrap();
            assert_eq!(saved.cwd, "/work/repo/src/nested");
            assert_eq!(saved.label, "Chosen name");
            view.start_kept_core_chat(Agent::Codex, None, None, true, cx).unwrap();
            assert_eq!(view.core_terminals.wanted.back().unwrap().2.cwd.as_deref(), Some(Path::new("/work/repo/src/nested")));
        })
    });
}

#[gpui::test]
fn a_chat_keeps_its_requested_group_while_the_editor_gets_focus(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, false);
    cx.update(|window, cx| view.update(cx, |view, cx| {
        let workspace = view.open_kept_workspace(None, Path::new("/repo"), core_tab("t1"), window, cx).unwrap();
        let scope = crate::browser::scope(&view.endpoints[0]);
        let chat_group = view.active_group().unwrap();
        view.split_group(chat_group, window, cx);
        let editor_group = view.active_group().unwrap();
        let editor = Store::update(cx, |store| store.open_editor_tab(scope, &workspace, crate::browser::Editor::VsCode, None)).unwrap();
        view.show_browser_tab_in(Some(editor_group), editor, window, cx);
        view.config.features.acp_chats = true;
        view.chats.program = crate::chat::Program::Found("/opt/sheprd-acp".into());
        // The launcher names its group even while another group is active.
        view.start_chat_in(Agent::Codex, chat_group, cx);
        let (purpose, host, _, _) = view.core_terminals.wanted.pop_back().unwrap();
        assert!(matches!(&purpose, Purpose::Tab { group: Some(group), .. } if *group == chat_group));
        view.activate_group(chat_group, window, cx);
        view.activate_group(editor_group, window, cx);
        let terminal = TerminalId::new("t-chat").unwrap();
        view.core_terminals.start_for_test(purpose, host, Ok(TerminalInfo {
            agent: AgentStatus::Unknown,
            cwd: Some(PathBuf::from("/repo")),
            foreground: None,
            id: terminal,
            labels: Default::default(),
            name: None,
            reporter: None,
            state: RunState::Running,
            title: None,
        }));
        view.place_started_core_terminal(window, cx);
        assert_eq!(view.group_shown(editor_group, cx), crate::browser::Shown::Page(editor));
        let crate::browser::Shown::Page(chat) = view.group_shown(chat_group, cx) else { panic!("chat tab missing"); };
        assert_ne!(chat, editor);
        assert!(matches!(&cx.global::<Store>().get(chat).unwrap().location, Some(crate::browser::Location::CoreTerminal { panes, .. }) if panes.first().as_str() == "t-chat"));
    }));
}

#[gpui::test]
fn a_background_chat_finishing_keeps_the_selected_tab(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, false);
    cx.update(|window, cx| view.update(cx, |view, cx| {
        let workspace = view.open_kept_workspace(None, Path::new("/repo"), core_tab("t1"), window, cx).unwrap();
        let group = view.active_group().unwrap();
        let shown = view.group_shown(group, cx);
        view.config.features.acp_chats = true;
        view.chats.program = crate::chat::Program::Found("/opt/sheprd-acp".into());
        view.start_kept_core_chat(Agent::Codex, None, None, false, cx).unwrap();
        let (purpose, host, _, _) = view.core_terminals.wanted.pop_back().unwrap();
        view.core_terminals.start_for_test(purpose, host, Ok(TerminalInfo {
            agent: AgentStatus::Unknown, cwd: None, foreground: None,
            id: TerminalId::new("t-background").unwrap(), labels: Default::default(),
            name: None, reporter: None, state: RunState::Running, title: None,
        }));
        view.place_started_core_terminal(window, cx);
        assert_eq!(view.group_shown(group, cx), shown);
        assert!(cx.global::<Store>().tabs().any(|tab| tab.workspace_id == workspace &&
            matches!(&tab.location, Some(crate::browser::Location::CoreTerminal { panes, .. }) if panes.first().as_str() == "t-background")));
    }));
}

#[gpui::test]
fn nested_checkout_names_and_live_folders_survive_last_close(cx: &mut TestAppContext) {
    for (linked, custom, expected) in [
        (false, false, "repo"),
        (true, false, "original-worktree"),
        (true, true, "My project"),
        (false, true, "My project"),
    ] {
        let (view, cx) = window(cx, false);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let id = view
                    .open_kept_workspace(
                        None,
                        Path::new("/original-worktree"),
                        core_tab("t1"),
                        window,
                        cx,
                    )
                    .unwrap();
                let scope = crate::browser::scope(&view.endpoints[0]);
                let tab = cx
                    .global::<Store>()
                    .in_workspace(&scope, &id)
                    .next()
                    .unwrap()
                    .id;
                if custom {
                    view.rename_kept_workspace(&id, "My project", cx);
                }
                let terminal = TerminalId::new("t1").unwrap();
                view.core_terminals.show_for_test(
                    tab,
                    crate::core_terminals::panes::Panes::single(terminal.clone()),
                    false,
                    vec![TerminalInfo {
                        agent: AgentStatus::Unknown,
                        cwd: Some("/work/repo/src/nested".into()),
                        foreground: None,
                        id: terminal,
                        labels: Default::default(),
                        name: None,
                        reporter: None,
                        state: RunState::Running,
                        title: None,
                    }],
                );
                view.core_terminals.seed_checkout(
                    "/work/repo/src/nested",
                    Some(sheprd_vcs::git::Identity {
                        branch: Some("feature".into()),
                        key: "/work/repo/.git".into(),
                        label: "repo".into(),
                        linked,
                        root: "/work/repo".into(),
                    }),
                );
                view.refresh_core_session(cx);
                let current = view.sessions(0).workspace(&id).unwrap().clone();
                assert_eq!(current.label, expected);
                assert_eq!(current.cwd, "/work/repo/src/nested");
                assert_eq!(current.branch.as_deref(), Some("feature"));
                assert_eq!(current.worktree.as_ref().unwrap().linked, linked);
                view.close_browser_tab(tab, window, cx);
                view.forget_core_tabs_for_test();
                view.refresh_core_session(cx);
                let restored = view.sessions(0).workspace(&id).unwrap().clone();
                assert_eq!(restored.label, expected);
                assert_eq!(restored.cwd, current.cwd);
                assert_eq!(restored.worktree, current.worktree);
                let saved = cx.global::<Store>().kept_workspace(&scope, &id).unwrap();
                assert_eq!(saved.cwd, current.cwd);
                assert_eq!(
                    saved.label,
                    if custom {
                        "My project"
                    } else {
                        "original-worktree"
                    }
                );
            })
        });
    }
}
