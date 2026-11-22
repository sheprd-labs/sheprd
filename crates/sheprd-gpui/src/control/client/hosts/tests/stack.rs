use super::*;
use crate::control::protocol::hosts::{LayerInfo, PrInfo, QueueInfo, StackInfo};

fn layer(position: u64, number: u64, status: &str) -> LayerInfo {
    LayerInfo {
        position,
        number,
        title: format!("Layer {position}"),
        status: status.into(),
    }
}

#[test]
fn a_stacked_queued_pull_request_says_what_blocks_it_and_its_place_in_the_queue() {
    let pr = PrInfo {
        number: 1364,
        url: "https://github.com/example/project/pull/1364".into(),
        title: "Sidebar".into(),
        state: "Open".into(),
        draft: false,
        head: "feat/t3-sidebar".into(),
        base: "feat/t3-client".into(),
        additions: 2,
        deletions: 0,
        changed_files: 2,
        review: None,
        merge_status: None,
        checks: Vec::new(),
        stack: Some(StackInfo {
            number: 950,
            base: "main".into(),
            size: 3,
            position: 3,
            verdict: "blocked".into(),
            ready: None,
            blocked_by: Some(1362),
            reason: Some("checks_failed".into()),
            layers: vec![
                layer(1, 1361, "merged"),
                layer(2, 1362, "checks_failed"),
                layer(3, 1364, "review_required"),
            ],
        }),
        queue: Some(QueueInfo {
            position: 2,
            state: "awaiting_checks".into(),
            eta_seconds: Some(61),
        }),
    };
    assert_eq!(
        text::pr(&pr),
        "#1364 Sidebar\n\
         Open · feat/t3-sidebar -> feat/t3-client · +2 -0 in 2 files\n\
         Checks: none reported\n\
         Stack: layer 3 of 3 onto main · blocked by #1362 (checks failed)\n  \
         #1361 merged · #1362 checks failed · #1364 review required\n\
         Queue: position 2 · awaiting checks · about 2 min\n\
         https://github.com/example/project/pull/1364"
    );
}
