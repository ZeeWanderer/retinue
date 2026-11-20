use retinue_ui::{authoring::*, drawing::DrawList, state::State};

#[derive(Clone, PartialEq)]
pub struct PaintView(pub State<DrawList>);

#[view]
impl View for PaintView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        Canvas(self.0.get())
    }
}
