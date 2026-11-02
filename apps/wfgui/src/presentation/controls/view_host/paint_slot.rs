use super::PaintSources;
use retinue_ui::{drawing::Typography, prelude::*, view::Primitive, view::PrimitiveOutput};

#[derive(PartialEq)]
pub(crate) struct PaintSlot(pub retinue_render::Key);

impl PaintSlot {
    pub fn new(key: retinue_render::Key) -> Self {
        Self(key)
    }
}

impl Primitive for PaintSlot {
    fn resolve(&self, cx: &mut ViewContext<'_>) -> PrimitiveOutput {
        let sources = cx
            .environment()
            .model::<PaintSources>()
            .expect("page paint host");
        let content = sources.slot(self.0).get();
        let mut output =
            PrimitiveOutput::new(content.drawing, cx.environment().get::<Typography>())
                .fill_available();
        output.cache = content.cache;
        output
    }
}

impl IntoElement for PaintSlot {
    fn into_element(self) -> Element {
        let key = self.0;
        Element::primitive(self).id(key)
    }
}
