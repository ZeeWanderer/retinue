use super::{composition::position, foundry_model::Model};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub struct OverlaysView {
    pub model: Model,
}

impl View for OverlaysView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let area = self.model.area.get();
        ui! {
            ZStack() {
                Canvas::new(self.model.overlays_paint.get())
                if let Some(bounds) = self.model.component_bounds.get() {
                    position(empty(), bounds).interactive()
                }
                for key in self.model.overlay_controls.order().keys() {
                    position(empty(), self.model.overlay_controls.get(*key).unwrap()).interactive().id(*key)
                }
            }.alignment(Alignment::Start).frame(area.width, area.height)
        }
    }
}
