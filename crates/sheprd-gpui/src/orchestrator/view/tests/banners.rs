//! A source failing the same way in several checkouts of one repository
//! shows one banner, not one per checkout.
use super::*;
use crate::orchestrator::{
    Error, Request, SourceStatus, SyncState, Timing,
    view::{RepoSpec, repo::Repo},
};
use std::sync::Arc;

#[gpui::test]
fn one_failure_shared_by_checkouts_is_one_banner(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    let count = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for name in ["second", "third"] {
                view.repos.push(Repo::start(RepoSpec {
                    key: name.into(),
                    host: "local".into(),
                    request: Request {
                        target: sheprd_client::ConnectTarget::Local,
                        checkout: dir.path().join(name).to_string_lossy().into_owned(),
                        workspace_id: name.into(),
                        token: None,
                        data_root: Some(dir.path().join(format!("{name}-data"))),
                        azure_store: Default::default(),
                        gitlab_store: Default::default(),
                        timing: Timing::default(),
                    },
                }));
            }
            let failed = Arc::new(Error::Busy);
            for repo in &mut view.repos {
                repo.snapshot.error = None;
                repo.snapshot.sources = vec![SourceStatus {
                    key: source(Provider::Github),
                    state: SyncState::Failed(failed.clone()),
                }];
            }
            // Another repository's failure still shows on its own.
            let mut other = source(Provider::Github);
            other.repository = "penso/other".into();
            view.repos[2].snapshot.sources.push(SourceStatus {
                key: other,
                state: SyncState::Failed(failed),
            });
            view.render_banners(cx).len()
        })
    });
    assert_eq!(count, 2);
}
