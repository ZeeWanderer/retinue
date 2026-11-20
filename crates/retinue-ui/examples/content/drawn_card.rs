use crate::model::{Model, text_style};
use retinue_render::{Bounds, Color, ImageFit, ImageSource};
use retinue_ui::{
    authoring::*,
    drawing::{
        CanvasContent, DrawingContext,
        text::{Paragraph, TextAlignment},
    },
    prelude::Typography,
};
use std::rc::Rc;

#[derive(PartialEq)]
pub struct DrawnCard {
    pub model: Model,
}

#[derive(PartialEq)]
struct Content {
    title: Rc<Paragraph>,
    image: Option<ImageSource>,
}

#[view]
impl View for DrawnCard {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let content = Content {
            title: Rc::new(Paragraph::new(self.model.title.get(), &text_style(), None)),
            image: self.model.artwork.read().pixels(),
        };
        let height = content
            .title
            .layout(160.0, TextAlignment::Center)
            .unwrap()
            .size
            .height;
        Canvas::new(content)
            .frame(Frame::new().width(184.0).height(104.0 + height))
            .environment::<Typography>(text_style())
            .padding(20.0)
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
        let label = self.title.layout(160.0, TextAlignment::Center).unwrap();
        let origin = label.origin(Bounds::new(12.0, 92.0, 160.0, label.size.height));
        // Match the composed view's placement boundary, not the glyph positions.
        context.translated(origin, |context| {
            label.draw(
                context,
                Bounds::new(0.0, 0.0, label.size.width, label.size.height),
                context.text_style.color,
            );
        });
    }
}
