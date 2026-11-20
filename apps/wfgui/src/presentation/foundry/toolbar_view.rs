use super::{composition::position, foundry_model::Model};
use retinue_ui::{authoring::*, layout::anchors::AnchorPoint};

#[derive(PartialEq)]
pub struct ToolbarView {
    pub model: Model,
}

#[retinue_ui::authoring::view]
impl View for ToolbarView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let rect = self.model.toolbar.get();
        position(
            ui! {
                ZStack() {
                    Canvas(self.model.toolbar_paint.get())
                    ForEach(self.model.controls.order().keys().iter().copied(), |key| *key) { |key|
                        position(empty(), self.model.controls.get(key).unwrap()).interactive().named(key)
                    }
                }.alignment(AnchorPoint::TOP_LEADING)
            },
            rect,
        )
    }
}
