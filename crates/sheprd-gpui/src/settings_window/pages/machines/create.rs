//! Settings > Virtual Macs, the first row: making a machine from Apple's
//! macOS, and how far that has got.
// i18n: migrated
use super::gigabytes;
use crate::{
    i18n::fl,
    machine::{
        Name,
        image::unused,
        install::{self, Progress},
    },
    settings_window::{SettingsWindow, ui},
};
use gpui::{prelude::*, *};

/// What the first machine is called, so `sheprd machine open` finds it
/// without a name; later ones are `default-2` and on.
const FIRST_NAME: &str = "default";

/// How tall the progress bar draws.
const BAR_HEIGHT: f32 = 4.;

impl SettingsWindow {
    /// Starts making a machine, named as [`new_name`] says.
    fn make_machine(&mut self, cx: &mut Context<Self>) {
        let taken = self.machines.listings.clone().unwrap_or_default();
        let failed = install::progress().map(|(name, _)| name);
        let name = new_name(self.machines.create_name.take(), failed, |text| {
            taken.iter().any(|listing| listing.name.as_str() == text)
        });
        if let Some(name) = name {
            install::begin(name, self.source.clone(), cx);
        }
        cx.notify();
    }

    /// Reads the folders again once an install ends, so its machine shows.
    pub(super) fn follow_install(&mut self, cx: &mut Context<Self>) {
        let working = install::progress()
            .is_some_and(|(_, progress)| !matches!(progress, Progress::Failed(_)));
        if self.machines.installing && !working {
            self.open_machines(cx);
        }
        self.machines.installing = working;
    }

    pub(super) fn render_create(&self, cx: &Context<Self>) -> Div {
        let button = |id: &str, label: String| ui::button(&self.theme, id.to_owned(), label, true);
        let start = |label: String| {
            button("machine-create", label)
                .on_click(cx.listener(|this, _, _, cx| this.make_machine(cx)))
        };
        let cancel = button(
            "machine-create-cancel",
            fl!("settings-machines-create-cancel"),
        )
        .on_click(cx.listener(|_, _, _, cx| {
            install::cancel();
            cx.notify();
        }));
        let Some((name, progress)) = install::progress() else {
            let title = match &self.machines.create_name {
                Some(name) => fl!("settings-machines-create-named", name = name.as_str()),
                None => fl!("settings-machines-create"),
            };
            return ui::row(&self.theme, "machine-create-row", title)
                .description(fl!("settings-machines-create-description"))
                .control(start(fl!("settings-machines-create-start")))
                .into_div();
        };
        let title = fl!("settings-machines-create-making", name = name.as_str());
        let (description, fraction) = match &progress {
            Progress::Fetching => (fl!("settings-machines-create-fetching"), None),
            Progress::Downloading {
                version,
                done,
                total,
            } => (
                fl!(
                    "settings-machines-create-downloading",
                    version = version.as_str(),
                    done = gigabytes(*done),
                    total = gigabytes(*total)
                ),
                Some(fraction(*done, *total)),
            ),
            Progress::Verifying { version } => (
                fl!(
                    "settings-machines-create-verifying",
                    version = version.as_str()
                ),
                None,
            ),
            Progress::Preparing { version } => (
                fl!(
                    "settings-machines-create-preparing",
                    version = version.as_str()
                ),
                None,
            ),
            Progress::Installing { version, fraction } => (
                fl!(
                    "settings-machines-create-installing",
                    version = version.as_str(),
                    percent = format!("{:.0}", fraction * 100.) // i18n: data
                ),
                Some(*fraction),
            ),
            Progress::Failed(message) => (message.clone(), None),
        };
        let row = ui::row(&self.theme, "machine-create-row", title).description(description);
        let row = match progress {
            Progress::Failed(_) => row
                .control(start(fl!("settings-machines-create-retry")))
                .control(
                    button(
                        "machine-create-dismiss",
                        fl!("settings-machines-create-dismiss"),
                    )
                    .on_click(cx.listener(|_, _, _, cx| install::dismiss(cx))),
                ),
            _ => row.control(cancel),
        };
        div()
            .flex()
            .flex_col()
            .child(row.into_div())
            .when_some(fraction, |page, fraction| {
                page.child(self.render_create_bar(fraction))
            })
    }

    fn render_create_bar(&self, fraction: f64) -> Div {
        div()
            .debug_selector(|| "machine-create-bar".into())
            .w_full()
            .h(px(BAR_HEIGHT))
            .mb(px(8.))
            .rounded_full()
            .bg(rgb(self.theme.active))
            .child(
                div()
                    .h_full()
                    .rounded_full()
                    .bg(rgb(self.theme.primary()))
                    .w(relative(fraction as f32)),
            )
    }
}

/// What a new machine is called: the name asked for, or the one a failed try
/// had, so Try again makes what its row names, or else [`FIRST_NAME`]; the
/// first of those `taken` does not hold, with a number added if it does.
pub(super) fn new_name(
    asked: Option<Name>,
    failed: Option<Name>,
    taken: impl Fn(&str) -> bool,
) -> Option<Name> {
    let base = asked
        .or(failed)
        .or_else(|| Name::try_from(FIRST_NAME.to_owned()).ok())?;
    unused(&base, taken)
}

/// How much of `total` is `done`, from 0 to 1; 0 while the total is unknown.
pub(super) fn fraction(done: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.;
    }
    (done as f64 / total as f64).clamp(0., 1.)
}
