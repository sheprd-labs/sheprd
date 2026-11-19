//! The checkpoints dialog's commit versions: each commit of the branch's own
//! that was amended, rebased, or reworded, with every earlier version, how it
//! was recognized, and whose turn it was. Comparing two is left to
//! `sheprd change diff`, which the section names.

use crate::{SheprdWindow, checkpoint::Changes, i18n::fl};
use gpui::{prelude::*, *};
use sheprd_vcs::evolution::{History, Marker, Match, Outcome, versions};

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

/// One earlier version: its name, how it was replaced, by whom, and when.
fn version_line(history: &History, marker: &Marker, now: i64) -> String {
    let how = match &marker.outcome {
        Outcome::Replaced { how, .. } => match how {
            Match::ChangeId => fl!("checkpoints-changes-how-change-id"),
            Match::Patch => fl!("checkpoints-changes-how-patch"),
            Match::Subject => fl!("checkpoints-changes-how-subject"),
        },
        Outcome::Dropped => fl!("checkpoints-changes-how-dropped"),
    };
    let sha = short(&marker.old);
    if history.is_pending(marker) {
        return fl!("checkpoints-changes-pending", sha = sha, how = how);
    }
    let age = crate::ago::text(crate::ago::since(marker.time, now));
    match &marker.by {
        Some(agents) => fl!(
            "checkpoints-changes-by",
            sha = sha,
            how = how,
            agents = agents.as_str(),
            age = age
        ),
        None => fl!(
            "checkpoints-changes-outside",
            sha = sha,
            how = how,
            age = age
        ),
    }
}

impl SheprdWindow {
    /// The section under the timeline; nothing while it loads or for a
    /// branch with no commits of its own.
    pub(super) fn render_change_history(
        &self,
        changes: &Changes,
        branch: &str,
        now: i64,
    ) -> Option<Div> {
        let theme = &self.theme;
        let muted = rgb(theme.muted);
        let section = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .pt(px(10.))
            .mt(px(4.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("checkpoints-changes-title")),
            );
        let history = match changes {
            Changes::Loading | Changes::Ready(None) => return None,
            Changes::Failed(error) => {
                return Some(
                    section.child(
                        div()
                            .debug_selector(|| "checkpoint-changes-error".into())
                            .text_color(crate::menu::danger(theme))
                            .child(error.clone()),
                    ),
                );
            }
            Changes::Ready(Some(history)) => history,
        };
        let mut section = section;
        let mut any = false;
        for commit in history.stack.iter().rev() {
            let earlier = versions(&history.markers, &commit.sha);
            if earlier.is_empty() {
                continue;
            }
            any = true;
            let id = format!("checkpoint-change-{}", short(&commit.sha));
            section = section.child(
                div()
                    .debug_selector(move || id)
                    .flex()
                    .gap(px(8.))
                    .min_w_0()
                    .child(
                        div()
                            .flex_none()
                            .text_color(muted)
                            .child(short(&commit.sha).to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(commit.subject.clone()),
                    )
                    .child(div().flex_none().text_color(muted).child(fl!(
                        "checkpoints-changes-versions",
                        count = earlier.len() + 1
                    ))),
            );
            for marker in earlier {
                section = section.child(
                    div()
                        .pl(px(16.))
                        .min_w_0()
                        .text_color(muted)
                        .child(version_line(history, marker, now)),
                );
            }
        }
        let dropped: Vec<&Marker> = history
            .markers
            .iter()
            .rev()
            .filter(|marker| marker.outcome == Outcome::Dropped)
            .collect();
        if !dropped.is_empty() {
            any = true;
            section = section.child(
                div()
                    .pt(px(4.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("checkpoints-changes-dropped")),
            );
            for marker in dropped {
                section = section.child(
                    div()
                        .pl(px(16.))
                        .min_w_0()
                        .text_color(muted)
                        .child(version_line(history, marker, now)),
                );
            }
        }
        Some(if any {
            section.child(
                div()
                    .pt(px(4.))
                    .text_color(muted)
                    .child(fl!("checkpoints-changes-hint")),
            )
        } else {
            section.child(
                div()
                    .debug_selector(|| "checkpoint-changes-none".into())
                    .text_color(muted)
                    .child(fl!("checkpoints-changes-none", branch = branch)),
            )
        })
    }
}
