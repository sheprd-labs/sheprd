//! Settings > Virtual Macs, images: what new machines start from, so a machine
//! set up once is not set up again.
// i18n: migrated
use super::{Deletion, gigabytes};
use crate::{
    i18n::{Localize as _, fl},
    machine::{
        self, Name,
        image::{self, Image},
    },
    settings_window::{SettingsWindow, ui},
};
use gpui::{prelude::*, *};

impl SettingsWindow {
    /// Keeps stopped machine `name` as an image named after it.
    pub(super) fn save_machine_image(&mut self, name: &Name, cx: &mut Context<Self>) {
        let taken = self.machines.images.clone().unwrap_or_default();
        let Some(target) = image::unused(name, |text| {
            taken.iter().any(|image| image.name.as_str() == text)
        }) else {
            return;
        };
        let this = cx.entity().downgrade();
        machine::save_image(name, target, cx, move |saved, cx| {
            let _ = this.update(cx, |this, cx| {
                this.machines.failure = saved.err().map(|error| error.localize());
                this.open_machines(cx);
            });
        });
    }

    /// Makes a machine from `image`, named after it.
    fn new_machine(&mut self, from: &Name, cx: &mut Context<Self>) {
        let taken = self.machines.listings.clone().unwrap_or_default();
        let Some(name) = image::unused(from, |text| {
            taken.iter().any(|listing| listing.name.as_str() == text)
                || Name::try_from(text.to_owned()).is_ok_and(|name| machine::install::making(&name))
        }) else {
            return;
        };
        let from = from.clone();
        let make = cx
            .background_executor()
            .spawn(async move { image::create(&name, &from) });
        cx.spawn(async move |this, cx| {
            let made = make.await;
            let _ = this.update(cx, |this, cx| {
                this.machines.failure = made.err().map(|error| error.localize());
                this.open_machines(cx);
            });
        })
        .detach();
    }

    fn delete_image(&mut self, name: &Name, cx: &mut Context<Self>) {
        self.machines.confirming = None;
        let name = name.clone();
        let remove = cx
            .background_executor()
            .spawn(async move { image::delete(&name) });
        cx.spawn(async move |this, cx| {
            let removed = remove.await;
            let _ = this.update(cx, |this, cx| {
                this.machines.failure = removed.err().map(|error| error.localize());
                this.open_machines(cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn render_image(&self, image: &Image, cx: &Context<Self>) -> Div {
        let name = &image.name;
        let id = |what: &str| format!("machine-image-{name}-{what}");
        let asking = self.machines.confirming.as_ref() == Some(&Deletion::Image(name.clone()));
        let button = |what: &str, label: String, act: fn(&mut Self, &Name, &mut Context<Self>)| {
            let target = name.clone();
            ui::button(&self.theme, id(what), label, true)
                .on_click(cx.listener(move |this, _, _, cx| act(this, &target, cx)))
        };
        let controls = div().flex().flex_row().gap(px(6.));
        if asking {
            let confirm = button(
                "delete-confirm",
                fl!("settings-machines-delete-confirm"),
                Self::delete_image,
            )
            .text_color(rgb(self.theme.palette[1]));
            let cancel = button(
                "delete-cancel",
                fl!("settings-machines-delete-cancel"),
                |this, _, cx| {
                    this.machines.confirming = None;
                    cx.notify();
                },
            );
            // The question wraps under the image's name, so the buttons keep
            // their place at the right.
            return ui::row(&self.theme, id("delete-ask"), name.to_string())
                .description(fl!(
                    "settings-machines-image-delete-ask",
                    name = name.as_str()
                ))
                .control(controls.child(confirm).child(cancel))
                .into_div();
        }
        let new = button("new", fl!("settings-machines-image-new"), Self::new_machine);
        let delete = button(
            "delete",
            fl!("settings-machines-delete-button"),
            |this, name, cx| {
                this.machines.confirming = Some(Deletion::Image(name.clone()));
                cx.notify();
            },
        );
        ui::row(&self.theme, id("row"), name.to_string())
            .description(fl!(
                "settings-machines-image-size",
                size = gigabytes(image.disk_bytes)
            ))
            .control(controls.child(new).child(delete))
            .into_div()
    }

    pub(super) fn render_images(&self, images: &[Image], cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .child(ui::section(
                &self.theme,
                &fl!("settings-machines-images"),
                None,
            ))
            .child(self.control_note(fl!("settings-machines-images-description")))
            .children(images.iter().map(|image| self.render_image(image, cx)))
    }
}
