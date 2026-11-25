//! Classic starts with GPUI's 16-point rem. Moonframe sets its window rem to
//! the UI font size. Keep native spacing (including deferred menus) in Classic
//! units while Luau widgets continue to use the toolkit's scale.
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window, px,
};

pub(super) struct NativeRegion(pub AnyElement);

const CLASSIC_REM: f32 = 16.;

impl IntoElement for NativeRegion {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for NativeRegion {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (
            window.with_rem_size(Some(px(CLASSIC_REM)), |window| {
                self.0.request_layout(window, cx)
            }),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(px(CLASSIC_REM)), |window| {
            self.0.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(px(CLASSIC_REM)), |window| self.0.paint(window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::NativeRegion;
    use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, prelude::*, px};

    struct Fixture;

    impl Render for Fixture {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            window.set_rem_size(px(12.));
            let square = |name: &'static str| div().debug_selector(move || name.into()).size_4();
            div()
                .flex()
                .child(NativeRegion(square("native").into_any_element()))
                .child(square("layout"))
        }
    }

    #[gpui::test]
    fn native_spacing_is_scoped_to_the_hosted_region(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Fixture);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("native").map(|b| b.size.width),
            Some(px(16.))
        );
        assert_eq!(
            cx.debug_bounds("layout").map(|b| b.size.width),
            Some(px(12.))
        );
    }
}
