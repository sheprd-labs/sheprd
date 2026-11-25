//! Agent requests that need a user's decision in Virtual Mac settings.
use super::Name;
use crate::{
    SheprdWindow,
    i18n::fl,
    remote::{DialogTone, dialog_button},
    settings_window::{self, MachineDeletion},
    window::{NoticeCard, chrome_focus::Region},
};
use gpui::{prelude::*, *};

#[derive(PartialEq, Eq)]
pub(crate) enum Request {
    Create(Option<Name>),
    Delete(MachineDeletion),
}

impl SheprdWindow {
    pub(crate) fn offer_machine_request(
        &mut self,
        request: Request,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(pending) = &self.menu.machine_settings_request {
            return pending == &request;
        }
        self.menu.machine_settings_request = Some(request);
        cx.notify();
        true
    }

    pub(crate) fn machine_settings_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let request = self.menu.machine_settings_request.as_ref()?;
        let label = match request {
            Request::Create(Some(name)) => {
                fl!("settings-machines-create-named", name = name.to_string())
            }
            Request::Create(None) => fl!("settings-machines-create"),
            Request::Delete(MachineDeletion::Machine(name) | MachineDeletion::Image(name)) => {
                fl!("settings-machines-review-deletion", name = name.to_string())
            }
        };
        let control = dialog_button(
            &self.theme,
            "review-machine-settings",
            label,
            DialogTone::Primary,
            true,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            cx.stop_propagation();
            let source = cx.entity().downgrade();
            match this.menu.machine_settings_request.take() {
                Some(Request::Create(name)) => settings_window::offer_new_machine(source, name, cx),
                Some(Request::Delete(deletion)) => {
                    settings_window::open_machines(source, Some(deletion), cx)
                }
                None => return,
            }
            cx.notify();
        }));
        let dismiss = dialog_button(
            &self.theme,
            "dismiss-machine-settings",
            fl!("settings-machines-create-dismiss"),
            DialogTone::Plain,
            true,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            cx.stop_propagation();
            this.menu.machine_settings_request = None;
            cx.notify();
        }));
        Some(
            div()
                .flex()
                .gap_1()
                .occlude()
                .children(
                    self.overlay_probe(crate::browser::Overlay::Notice(
                        NoticeCard::MachineSettings,
                    )),
                )
                .child(self.chrome_stop(Region::Notices, control))
                .child(self.chrome_stop(Region::Notices, dismiss))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{NoticeCard, Request};

    #[gpui::test]
    fn a_vm_install_request_leaves_the_keyboard_in_place(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let original = cx.focus_handle();
                window.focus(&original, cx);
                assert!(view.offer_machine_request(Request::Create(None), cx));
                assert!(original.is_focused(window));
                assert!(view.notice_shown(NoticeCard::MachineSettings));
                assert!(view.menu.page.is_none());
            })
        });
    }
}
