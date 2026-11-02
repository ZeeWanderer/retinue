use crate::model::{Model, text_style};
use retinue_render::{Bounds, ImageFit, ImageSource};
use retinue_ui::{
    drawing::text::{Paragraph, TextAlignment},
    prelude::*,
};

#[derive(PartialEq)]
pub struct DrawnCard(pub Model);

#[derive(PartialEq)]
struct Content {
    title: String,
    image: Option<ImageSource>,
}

impl View for DrawnCard {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let content = Content {
            title: self.0.title.get(),
            image: self.0.artwork.read().pixels(),
        };
        let label = Paragraph::new(content.title.as_str(), &text_style(), None).layout(160.0);
        Canvas::new(content)
            .frame(184.0, 104.0 + label.size.height)
            .environment::<Typography>(text_style())
    }
}

impl CanvasContent for Content {
    fn draw(&self, context: &mut DrawingContext) {
        context.fill(context.bounds, 8.0, Color::rgb(0x252b30));
        let art = Bounds::new(12.0, 12.0, 160.0, 72.0);
        context.fill(art, 4.0, Color::rgb(0x343c43));
        if let Some(image) = &self.image {
            context.image(image, art, ImageFit::Contain);
        }
        let paragraph = Paragraph::new(self.title.as_str(), &context.text_style, None);
        let label = paragraph.layout(160.0);
        label.draw(
            context,
            Bounds::new(12.0, 92.0, 160.0, label.size.height),
            context.text_style.color,
            TextAlignment::Center,
        );
    }
}
