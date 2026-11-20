use super::{composition::position, foundry_model::Model};
use crate::presentation::widgets::compiled::frame;
use retinue_ui::{authoring::*, layout::anchors::AnchorPoint};

#[derive(PartialEq)]
pub struct OverlaysView {
    pub model: Model,
}

#[retinue_ui::authoring::view]
impl View for OverlaysView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let area = self.model.area.get();
        ui! {
            ZStack() {
                Canvas::new(self.model.overlays_paint.get())
                if let Some(bounds) = self.model.component_bounds.get() {
                    position(empty(), bounds).interactive()
                }
                ForEach(self.model.overlay_controls.order().keys().iter().copied(), |key| *key) { |key|
                    position(empty(), self.model.overlay_controls.get(key).unwrap()).interactive().named(key)
                }
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(area.width, area.height))
        }
    }
}
