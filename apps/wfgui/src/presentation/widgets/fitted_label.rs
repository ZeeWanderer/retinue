use retinue_render::{
    Bounds, Color, Command, TextAlignment, painter::CachedFont, painter::text_width,
};
use retinue_ui::{
    drawing::CanvasContent, drawing::DrawingContext, drawing::TextStyle, prelude::*,
    view::Primitive, view::PrimitiveOutput,
};
use std::rc::Rc;

pub(in crate::presentation) fn fit_text(
    font: &CachedFont,
    mut text: String,
    size: f32,
    width: f32,
) -> String {
    if text_width(font, &text, size) > width + 0.5 {
        let ends = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>();
        let index = ends.partition_point(|&end| {
            text_width(font, &format!("{}...", &text[..end]), size) <= width
        });
        text.truncate(ends[index.saturating_sub(1)]);
        text.push_str("...");
    }
    text
}

#[derive(Clone, PartialEq)]
struct FittedLabel {
    text: String,
    style: TextStyle,
    alignment: f32,
    scale: f32,
}

impl CanvasContent for FittedLabel {
    fn draw(&self, cx: &mut DrawingContext) {
        let width = (cx.bounds.width * self.scale).round();
        let inset = ((width
            - text_width(&self.style.font, &self.text, self.style.size * self.scale).ceil())
        .max(0.0)
            * self.alignment)
            .round();
        cx.emit(Command::Text {
            bounds: Bounds::new(
                inset / self.scale,
                0.0,
                (width - inset) / self.scale,
                cx.bounds.height,
            ),
            text: fit_text(
                &self.style.font,
                self.text.clone(),
                self.style.size * self.scale,
                width - inset,
            ),
            font: self.style.font.clone(),
            size: self.style.size,
            color: self.style.color,
            align: TextAlignment::Center,
        });
    }
}

impl Primitive for FittedLabel {
    fn resolve(&self, _: &mut ViewContext<'_>) -> PrimitiveOutput {
        PrimitiveOutput::new(self.clone(), self.style.clone()).fill_available()
    }
}

pub fn fitted_label(
    text: impl Into<String>,
    font: Rc<CachedFont>,
    size: f32,
    color: [u8; 4],
    center: bool,
    scale: f32,
) -> Element {
    aligned_label(
        text,
        font,
        size,
        color,
        if center { 0.5 } else { 0.0 },
        scale,
    )
}

pub fn aligned_label(
    text: impl Into<String>,
    font: Rc<CachedFont>,
    size: f32,
    color: [u8; 4],
    alignment: f32,
    scale: f32,
) -> Element {
    Element::primitive(FittedLabel {
        text: text.into(),
        style: TextStyle::new(font, size, Color::from_bgra(color)),
        alignment,
        scale,
    })
}
