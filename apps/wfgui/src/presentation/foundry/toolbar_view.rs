use super::{composition::position, foundry_model::Model};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub struct ToolbarView {
    pub model: Model,
}

impl View for ToolbarView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let rect = self.model.toolbar.get();
        position(
            ui! {
                ZStack() {
                    Canvas(self.model.toolbar_paint.get())
                    for &key in self.model.controls.order().keys() {
                        position(empty(), self.model.controls.get(key).unwrap()).interactive().id(key)
                    }
                }.alignment(Alignment::Start)
            },
            rect,
        )
    }
}
