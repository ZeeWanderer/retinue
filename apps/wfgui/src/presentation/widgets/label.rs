use retinue_render::{
    Bounds, Color, Command, TextAlignment, painter::CachedFont, painter::text_width,
};
use retinue_ui::{prelude::*, view::Primitive, view::PrimitiveOutput};
use std::rc::Rc;

#[derive(Clone, PartialEq)]
struct Label {
    text: String,
    style: TextStyle,
    center: bool,
}

impl CanvasContent for Label {
    fn draw(&self, cx: &mut DrawingContext) {
        let inset = if self.center {
            ((cx.bounds.width - text_width(&self.style.font, &self.text, self.style.size)) / 2.0)
                .max(0.0)
        } else {
            0.0
        };
        cx.emit(Command::Text {
            bounds: Bounds::new(inset, 0.0, cx.bounds.width - inset, cx.bounds.height),
            text: self.text.clone(),
            font: self.style.font.clone(),
            size: self.style.size,
            color: self.style.color,
            align: TextAlignment::Center,
        });
    }
}

impl Primitive for Label {
    fn resolve(&self, _: &mut ViewContext<'_>) -> PrimitiveOutput {
        PrimitiveOutput::new(self.clone(), self.style.clone()).fill_available()
    }
}

pub fn label(
    text: impl Into<String>,
    font: Rc<CachedFont>,
    size: f32,
    color: [u8; 4],
    center: bool,
) -> Element {
    Element::primitive(Label {
        text: text.into(),
        style: TextStyle::new(font, size, Color::from_bgra(color)),
        center,
    })
}
