use retinue_ui::{
    drawing::DrawList, drawing::Typography, prelude::*, view::Primitive, view::PrimitiveOutput,
};

#[derive(Clone, PartialEq)]
pub struct PaintView(pub State<DrawList>);

impl PaintView {
    pub fn new(paint: State<DrawList>) -> Self {
        Self(paint)
    }
}

impl Primitive for PaintView {
    fn resolve(&self, cx: &mut ViewContext<'_>) -> PrimitiveOutput {
        PrimitiveOutput::new(self.0.get(), cx.environment().get::<Typography>()).fill_available()
    }
}

impl IntoElement for PaintView {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}
