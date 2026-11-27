//! Native captures with synthetic sessions; never reads a person's transcripts.
use super::*;
use anyhow::{Context as _, Result, ensure};
use chrono::{Days, Utc};
use service::Snapshot;
use sheprd_client::herdr::AgentKind;
use sheprd_control_protocol::protocol::app::TraceFilter;
use sheprd_traces::{Activity, EventKind, Session, Visibility};
use std::{sync::Arc, time::Duration};

pub(crate) fn start(handle: WindowHandle<SheprdWindow>, cx: &mut App) {
    cx.spawn(async move |cx| match run(handle, cx).await {
        Ok(()) => {
            eprintln!("TRACES native PASS");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("TRACES native FAIL: {error:#}");
            std::process::exit(1);
        }
    })
    .detach();
}

async fn run(handle: WindowHandle<SheprdWindow>, cx: &mut AsyncApp) -> Result<()> {
    let snapshot = Arc::new(snapshot());
    // i18n: data
    for (theme, label) in [("Catppuccin Mocha", "dark"), ("Catppuccin Latte", "light")] {
        for width in [1400., 640.] {
            handle.update(cx, |view, window, cx| -> Result<()> {
                view.config.theme = theme.into();
                view.theme = view.config.theme(label == "light")?;
                view.config.sidebar_style.activity_bar = true;
                view.activity.panel = ActivityPanel::Traces;
                view.traces.status = Status::Ready(snapshot.clone());
                view.traces.peers = vec![
                    replication::Report {
                        host: "Buildbox".into(), // i18n: data
                        state: replication::Phase::Ready,
                    }, // i18n: data
                    replication::Report {
                        host: "Studio".into(), // i18n: data
                        state: replication::Phase::Syncing,
                    }, // i18n: data
                ];
                view.ensure_traces_page(cx);
                if let Some(page) = &mut view.traces.page {
                    // Keep both activity charts visible on shorter guest displays.
                    page.scroll.set_offset(point(px(0.), px(-255.)));
                    page.overlap
                        .fixture(example_overlap(snapshot.sessions.iter()));
                }
                window.resize(size(px(width), px(900.)));
                cx.notify();
                Ok(())
            })??;
            capture(handle, &format!("{label}-{width}"), cx).await?;
        }
    }
    for (scope, label) in [
        (TraceFilter::Tag { tag: "work".into() }, "tag-work"),
        (
            TraceFilter::Host {
                id: "demo-host-0".into(),
            },
            "host-workstation",
        ),
    ] {
        handle.update(cx, |view, window, cx| -> Result<()> {
            view.config.theme = "Catppuccin Mocha".into(); // i18n: data
            view.theme = view.config.theme(false)?;
            view.set_trace_filter(scope.clone(), cx);
            if let Some(page) = &mut view.traces.page {
                page.activity = Some(Ok(activity(&snapshot, &scope)));
                page.overlap.fixture(example_overlap(
                    snapshot
                        .sessions
                        .iter()
                        .filter(|s| scope.includes(&s.host_id, &snapshot.hosts)),
                ));
            }
            window.resize(size(px(1400.), px(1000.)));
            cx.notify();
            Ok(())
        })??;
        capture(handle, label, cx).await?;
    }
    handle.update(cx, |view, window, cx| -> Result<()> {
        view.config.theme = "Catppuccin Mocha".into(); // i18n: data
        view.theme = view.config.theme(false)?;
        view.set_trace_filter(TraceFilter::All, cx);
        let page = view.traces.page.as_mut().context("missing traces page")?;
        page.overlap
            .fixture(example_overlap(snapshot.sessions.iter()));
        page.selected = Some(snapshot.sessions[0].id.clone());
        page.detail = Some(Detail::Loaded(events()));
        window.resize(size(px(1400.), px(1000.)));
        cx.notify();
        Ok(())
    })??;
    capture(handle, "detail", cx).await?;
    Ok(())
}

async fn capture(handle: WindowHandle<SheprdWindow>, label: &str, cx: &mut AsyncApp) -> Result<()> {
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    AnyWindowHandle::from(handle).update(cx, |root, window, cx| -> Result<()> {
        let view = root
            .downcast::<SheprdWindow>()
            .map_err(|_| anyhow::anyhow!("unexpected root"))?;
        ensure!(view.read(cx).activity_section() == Some(ActivityPanel::Traces));
        window.draw(cx).clear(cx);
        // The explicit draw above consumes the dirty scene without presenting
        // it through X11's frame callback. Schedule that presentation too.
        window.refresh();
        Ok(())
    })??;
    // X11 captures read the presented window, not GPUI's scene. Let the
    // refreshed frame reach the compositor before reading that window.
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    #[cfg(any(feature = "mockup", target_os = "linux"))]
    if let Some(path) = std::env::var_os("HERDR_TEST_TRACES_CAPTURE") {
        let path = std::path::PathBuf::from(path).with_extension(format!("{label}.png"));
        ensure!(path.is_absolute(), "capture destination must be absolute");
        #[cfg(target_os = "linux")]
        cx.background_executor()
            .spawn(async move {
                let status = sheprd_client::new_command("python3")
                    .arg(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../scripts/capture-native-x11.py"
                    ))
                    .arg(std::process::id().to_string())
                    .arg(path)
                    .status()
                    .context("capture native X11 window")?;
                ensure!(status.success(), "native X11 capture failed");
                Ok::<_, anyhow::Error>(())
            })
            .await?;
        #[cfg(all(feature = "mockup", not(target_os = "linux")))]
        {
            let image = AnyWindowHandle::from(handle)
                .update(cx, |_, window, _| window.render_to_image())??;
            cx.background_executor()
                .spawn(async move { image.save(&path).context("save native Traces capture") })
                .await?;
        }
    }
    #[cfg(not(any(feature = "mockup", target_os = "linux")))]
    let _ = label;
    Ok(())
}

fn snapshot() -> Snapshot {
    let now = Utc::now();
    let agents = [
        AgentKind::Claude,
        AgentKind::Codex,
        AgentKind::Pi,
        AgentKind::Omp,
    ];
    let titles = [
        "Fix login redirects and cover expired sessions", // i18n: data
        "Add a health check to the API",                  // i18n: data
        "Keep the navigation header visible while scrolling", // i18n: data
        "Review database migrations before deployment",   // i18n: data
        "Improve keyboard navigation in the settings dialog", // i18n: data
        "Document the release checklist",                 // i18n: data
    ];
    let sessions = titles
        .iter()
        .enumerate()
        .map(|(i, title)| Session {
            id: format!("demo-{i}"),
            host_id: format!("demo-host-{}", i % 3),
            host: ["Workstation", "Buildbox", "Studio"][i % 3].into(), // i18n: data
            agent: agents[i % agents.len()],
            native_id: format!("demo-{i}"),
            user: "demo@workstation".into(),
            title: Some((*title).into()),
            model: Some(
                if i % 2 == 0 {
                    "claude-opus-4-6"
                } else {
                    "gpt-5.4"
                }
                .into(),
            ),
            cwd: Some("~/code/example-app".into()),
            branch: Some("fix/session-redirects".into()),
            agent_version: None,
            started_at: Some(now.timestamp_millis() - 3_600_000 * (i as i64 + 1)),
            updated_at: Some(now.timestamp_millis() - 60_000 * (i as i64 + 1)),
            messages: 24 + i as i64 * 13,
            user_messages: 5,
            tool_calls: 12 + i as i64 * 4,
            errors: i as i64 % 2,
            tokens_in: 12_400 + i as i64 * 8_700,
            tokens_out: 3_200 + i as i64 * 900,
            visibility: Visibility::Private,
        })
        .collect();
    let hosts = [
        ("Workstation", "personal"), // i18n: data
        ("Buildbox", "work"),        // i18n: data
        ("Studio", "work"),          // i18n: data
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (label, tag))| sheprd_client::device_tags::TaggedHost {
        id: format!("demo-host-{i}"),
        label: label.into(),
        tags: sheprd_client::device_tags::ComputerTags::new([tag], 1),
    })
    .collect();
    let mut snapshot = Snapshot {
        sessions,
        hosts,
        ..Snapshot::default()
    };
    snapshot.activity = activity(&snapshot, &TraceFilter::All);
    snapshot
}

fn activity(snapshot: &Snapshot, scope: &TraceFilter) -> Vec<Activity> {
    let today = Utc::now().date_naive();
    let agents = model::agents(&snapshot.sessions, &snapshot.activity);
    let mut activity = Vec::new();
    for day in 0..30 {
        for (h, host) in snapshot
            .hosts
            .iter()
            .enumerate()
            .filter(|(_, host)| scope.includes(&host.id, &snapshot.hosts))
        {
            for (i, agent) in agents.iter().enumerate().filter(|(_, agent)| {
                snapshot
                    .sessions
                    .iter()
                    .any(|s| s.host_id == host.id && s.agent == **agent)
            }) {
                if let Some(date) = today.checked_sub_days(Days::new(day)) {
                    activity.push(Activity {
                        day: date.to_string(),
                        agent: *agent,
                        sessions: 1,
                        messages: ((day as i64 * 7 + i as i64 * 11 + h as i64 * 5) % 43) + 3,
                    });
                }
            }
        }
    }
    activity
}

fn events() -> Vec<Event> {
    [
        (EventKind::User, None, "Fix login redirects when a session expires. Add a regression test."), // i18n: data
        (EventKind::Thinking, None, "Check the redirect target before refreshing the session."), // i18n: data
        (EventKind::ToolCall, Some("shell"), "cargo test session_redirect"), // i18n: data
        (EventKind::ToolResult, Some("shell"), "3 tests passed; 0 failed"), // i18n: data
        (EventKind::Agent, None, "The redirect now preserves the original route. Tests cover expired sessions and nested routes."), // i18n: data
    ].into_iter().enumerate().map(|(i, (kind, tool, text))| Event {
        seq: i as i64, at: None, kind, tool: tool.map(str::to_owned), text: text.into(),
    }).collect()
}

fn example_overlap<'a>(sessions: impl Iterator<Item = &'a Session>) -> sheprd_traces::Overlap {
    let recent_by_agent = sheprd_traces::ActiveAgent::counts(sessions.map(|s| s.agent));
    let sessions: usize = recent_by_agent.iter().map(|entry| entry.count).sum();
    let now = Utc::now();
    let daily = (0..30)
        .rev()
        .map(|i| sheprd_traces::OverlapPoint {
            at: (now - Days::new(i)).timestamp_millis(),
            count: if i == 0 {
                sessions
            } else {
                (i as usize + i as usize / 3) % (sessions + 1)
            },
        })
        .collect();
    sheprd_traces::Overlap {
        recent_by_agent,
        daily,
        recent: sheprd_traces::OverlapPoint {
            at: now.timestamp_millis(),
            count: sessions,
        },
    }
}
