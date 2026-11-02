use super::{CHROME, VIEWPORT};
use crate::{presentation::widgets::place, presentation::widgets::slot};
use retinue_render::Bounds;
use retinue_ui::{layout::anchors::AnchorPoint, prelude::*};

#[derive(Clone, Copy, Default, PartialEq)]
pub(in crate::presentation::settings) struct Viewport {
    pub width: f32,
    pub height: f32,
    pub bounds: Bounds,
    pub offset: f32,
}

#[derive(PartialEq)]
pub(in crate::presentation::settings) struct SettingsView {
    pub content: Element,
    pub viewport: State<Viewport>,
}

impl View for SettingsView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let v = self.viewport.get();
        ui! {
            ZStack() {
                place(ScrollViewport::new(self.content.clone()).position(0.0, v.offset), v.bounds).id(VIEWPORT)
                place(slot(CHROME), Bounds::new(0.0, 0.0, v.width, v.height))
            }.alignment(Alignment::Start).frame_aligned(v.width, v.height, AnchorPoint::TOP_LEADING)
        }
    }
}
