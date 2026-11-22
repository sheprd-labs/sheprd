//! Agents whose Overview row the user settled or snoozed leave the badge
//! while that hold lasts, as they leave the rows to look at.

use super::*;
use crate::overview::{Key, agent_key};
use std::time::Duration;

/// The key the Overview gives `state`'s agent `index` on `endpoint`.
fn key_of(endpoint: &Endpoint, state: &ClientShellSnapshot, index: usize) -> Key {
    agent_key(&endpoint.id, state, &state.agents[index])
}

/// Agent p0 done and p1 blocked, as two rows.
fn needing() -> ClientShellSnapshot {
    let mut state = snapshot(2);
    state.agents[0].agent_status = AgentStatus::Done;
    state.agents[1].agent_status = AgentStatus::Blocked;
    state
}

#[gpui::test]
fn a_held_agent_leaves_the_badge_until_its_snooze_ends(cx: &mut gpui::TestAppContext) {
    let id = cx.add_window(fixture_window).window_id();
    let state = Arc::new(needing());
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let wakes = now + Duration::from_secs(60);
    let mut marks = Marks::default();
    let mut badge = Badge::default();
    let update = |badge: &mut Badge, marks: &Marks, at: SystemTime| {
        let changed = badge.windows.entry(id).or_default().update(
            std::iter::once((None, &state, local())),
            None,
            Some(marks),
            at,
        );
        badge.publish();
        changed
    };
    assert!(update(&mut badge, &marks, now));
    assert_eq!(badge.published, Some(2), "nothing held yet");

    assert!(marks.settle(&key_of(local(), &state, 0), State::Done));
    assert!(update(&mut badge, &marks, now), "a new mark is a change");
    assert_eq!(badge.published, Some(1), "the settled agent is aside");

    assert!(marks.snooze(&key_of(local(), &state, 1), State::Blocked, wakes));
    assert!(update(&mut badge, &marks, now));
    assert_eq!(badge.published, Some(0), "the snoozed agent is aside");

    assert!(
        !update(&mut badge, &marks, wakes - Duration::from_secs(1)),
        "nothing to recount before the snooze ends"
    );
    assert_eq!(badge.published, Some(0));
    assert!(
        update(&mut badge, &marks, wakes),
        "the snooze ending is a change though no mark or snapshot is"
    );
    assert_eq!(badge.published, Some(1), "the woken agent counts again");
    assert!(!update(&mut badge, &marks, wakes + Duration::from_secs(60)));
    assert_eq!(badge.published, Some(1), "the settled one stays aside");
}

#[gpui::test]
fn settling_and_releasing_a_row_republishes_the_count(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            let state = Arc::new(needing());
            view.endpoints[0].live.snapshot = Some(state.clone());
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));

            // The same snapshot: only the marks changed.
            let done = key_of(&view.endpoints[0], &state, 0);
            assert!(Marks::update(cx, |marks| marks.settle(&done, State::Done)));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
            let blocked = key_of(&view.endpoints[0], &state, 1);
            let later = SystemTime::now() + Duration::from_secs(3_600);
            assert!(Marks::update(cx, |marks| {
                marks.snooze(&blocked, State::Blocked, later)
            }));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(0));

            assert!(Marks::update(cx, |marks| marks.release(&done)));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
            assert!(Marks::update(cx, |marks| marks.release(&blocked)));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));

            // A pin holds nothing back.
            assert!(Marks::update(cx, |marks| marks.toggle_pin(&done)));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));
        })
        .unwrap();
}

#[gpui::test]
fn a_settled_agent_counts_again_once_its_state_changes(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            let mut state = needing();
            state.agents[1].agent_status = AgentStatus::Idle;
            let show = |view: &mut crate::SheprdWindow, state: &ClientShellSnapshot| {
                let state = Arc::new(state.clone());
                for live in [&mut view.live, &mut view.endpoints[0].live] {
                    live.snapshot = Some(state.clone());
                    live.status = ConnectionStatus::Connected;
                }
            };
            show(view, &state);
            let key = key_of(&view.endpoints[0], &state, 0);
            assert!(Marks::update(cx, |marks| marks.settle(&key, State::Done)));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(0));

            // Waiting on the user ends the hold at once.
            state.revision += 1;
            state.agents[0].agent_status = AgentStatus::Blocked;
            state.agents[0].state_change_seq += 1;
            show(view, &state);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
            // The window's tick polls the Overview too, which ends the mark.
            view.poll_overview(cx);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));

            // Settled again, then it works and finishes: the Overview's
            // tick sees it working and brings the row back, and so the badge.
            state.revision += 1;
            state.agents[0].agent_status = AgentStatus::Done;
            state.agents[0].state_change_seq += 1;
            show(view, &state);
            assert!(Marks::update(cx, |marks| marks.settle(&key, State::Done)));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(0));
            state.revision += 1;
            state.agents[0].agent_status = AgentStatus::Working;
            state.agents[0].state_change_seq += 1;
            show(view, &state);
            view.poll_overview(cx);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(0));
            state.revision += 1;
            state.agents[0].agent_status = AgentStatus::Done;
            state.agents[0].state_change_seq += 1;
            show(view, &state);
            view.poll_overview(cx);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
        })
        .unwrap();
}

#[gpui::test]
fn a_hold_on_one_host_leaves_the_same_ids_on_another_counted(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            let mut state = snapshot(2);
            state.agents[0].agent_status = AgentStatus::Done;
            let state = Arc::new(state);
            view.endpoints[0].live.snapshot = Some(state.clone());
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            let mut remote = Endpoint::new(
                "remote".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "box".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = Some(state.clone());
            remote.live.status = ConnectionStatus::Connected;
            view.endpoints.push(remote);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2), "one per host");
            let key = key_of(&view.endpoints[1], &state, 0);
            assert!(Marks::update(cx, |marks| marks.settle(&key, State::Done)));
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
        })
        .unwrap();
}

/// A Claude Code agent in herdr-core terminal `id`, working in `cwd`.
fn core_info(id: &str, agent: AgentStatus, cwd: &str) -> sheprd_backend::TerminalInfo {
    use sheprd_backend::{Process, RunState};

    sheprd_backend::TerminalInfo {
        agent,
        cwd: Some(cwd.into()),
        foreground: Some(Process {
            argv: vec!["claude".to_owned()],
            name: "claude".to_owned(),
            pid: 4,
        }),
        id: TerminalId::new(id).unwrap(),
        labels: Default::default(),
        name: None,
        reporter: None,
        state: RunState::Running,
        title: None,
    }
}

#[gpui::test]
fn a_held_core_row_leaves_the_badge_through_the_window_count(cx: &mut gpui::TestAppContext) {
    use crate::{browser::TabId, core_terminals::panes::Panes};

    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            let t1 = TerminalId::new("t1").unwrap();
            let mut panes = Panes::single(t1.clone());
            panes
                .split(
                    &t1,
                    crate::core_terminals::panes::Axis::Row,
                    TerminalId::new("t2").unwrap(),
                )
                .unwrap();
            let done = core_info("t1", AgentStatus::Done, "/work/herdr");
            view.core_terminals.show_for_test(
                TabId::test(7),
                panes,
                false,
                vec![
                    done.clone(),
                    core_info("t2", AgentStatus::Blocked, "/work/other"),
                ],
            );
            view.count_core_agents(id, true, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));
            view.count_core_agents(id, false, cx);
            assert_eq!(cx.global::<Badge>().published, Some(0), "no core, no count");

            // The row the Overview lists for t1, settled there.
            let key = crate::overview::core_agent_key(&done).unwrap();
            let rows = view.overview_rows(cx);
            assert!(rows.iter().any(|row| row.key == key));
            assert!(Marks::update(cx, |marks| marks.settle(&key, State::Done)));
            view.count_core_agents(id, true, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
            assert!(Marks::update(cx, |marks| marks.release(&key)));
            view.count_core_agents(id, true, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));
        })
        .unwrap();
}

#[gpui::test]
fn core_holds_apply_to_listed_rows_and_are_found_again_only_on_change(
    cx: &mut gpui::TestAppContext,
) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |_, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            // The stand-in shell and a tab run Claude Code in one folder, so
            // both name the same row; only the tab's is one the Overview lists.
            let info = core_info("t1", AgentStatus::Done, "/work/herdr");
            let key = crate::overview::core_agent_key(&info).unwrap();
            let agent = |terminal: &str, listed| CoreAgent {
                terminal: TerminalId::new(terminal).unwrap(),
                status: AgentStatus::Done,
                listed,
            };
            let agents = vec![agent("t1", true), agent("shell", false)];
            let asked = std::cell::Cell::new(0);
            let count = |lists, cx: &mut App| {
                sync_core(
                    id,
                    agents.clone(),
                    lists,
                    |_| {
                        asked.set(asked.get() + 1);
                        Some(key.clone())
                    },
                    cx,
                );
                cx.global::<Badge>().published
            };
            assert_eq!(count(1, cx), Some(2));
            assert_eq!(asked.get(), 0, "no marks, no key");

            assert!(Marks::update(cx, |marks| marks.settle(&key, State::Done)));
            assert_eq!(count(1, cx), Some(1), "the stand-in shell still counts");
            assert_eq!(asked.get(), 1, "only the listed agent is keyed");
            assert_eq!(count(1, cx), Some(1));
            assert_eq!(asked.get(), 1, "nothing changed, nothing is keyed");
            assert_eq!(count(2, cx), Some(1));
            assert_eq!(asked.get(), 2, "a new list may move a folder");
        })
        .unwrap();
}
