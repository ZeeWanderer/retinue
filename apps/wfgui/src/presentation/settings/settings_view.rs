use super::{CHROME, VIEWPORT};
use crate::presentation::widgets::compiled::{frame, place, slot};
use retinue_render::Bounds;
use retinue_ui::{authoring::*, layout::anchors::AnchorPoint, state::State};

#[derive(Clone, Copy, Default, PartialEq)]
pub(in crate::presentation::settings) struct Viewport {
    pub width: f32,
    pub height: f32,
    pub bounds: Bounds,
    pub offset: f32,
}

#[derive(PartialEq)]
pub(in crate::presentation::settings) struct SettingsView {
    pub content: State<super::content_view::ContentView>,
    pub viewport: State<Viewport>,
}

#[view]
impl View for SettingsView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let v = self.viewport.get();
        ui! {
            ZStack() {
                place(ScrollViewport(compose(self.content.get())).position(0.0, v.offset), v.bounds).named(VIEWPORT)
                place(slot(CHROME), Bounds::new(0.0, 0.0, v.width, v.height))
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(v.width, v.height))
        }
    }
}
