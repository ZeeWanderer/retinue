use crate::{
    data::foundry::Asset, domain::media::image_store::ImageHandle, presentation::style,
    presentation::widgets::AssetImage,
};
use retinue_render::{Color, ImageFit, ImageSource};
use retinue_ui::{
    drawing::CanvasContent, drawing::DrawingContext, drawing::Typography, prelude::*,
    view::Primitive, view::PrimitiveOutput,
};

#[cfg(test)]
mod tests;

#[derive(PartialEq)]
pub(crate) struct ComponentImage {
    pub asset: Option<Asset>,
    pub owned: bool,
    pub edge: u32,
    pub scale: f32,
}

impl Primitive for ComponentImage {
    fn resolve(&self, cx: &mut ViewContext<'_>) -> PrimitiveOutput {
        let inset = (2.0 * self.scale).round() as u32;
        let fit = if self
            .asset
            .as_ref()
            .is_some_and(|asset| asset.source == "builtin")
        {
            ImageFit::Contain
        } else {
            ImageFit::Circle
        };
        let image =
            AssetImage::new(self.asset.as_ref(), self.edge.saturating_sub(inset * 2)).resolve(cx);
        PrimitiveOutput::new(
            ComponentPaint {
                pixels: image.as_ref().and_then(ImageHandle::pixels),
                image,
                fit,
                inset: inset as f32 / self.scale,
                radius: (self.edge / 2) as f32 / self.scale,
                owned: self.owned,
                scale: self.scale,
            },
            cx.environment().get::<Typography>(),
        )
        .fill_available()
    }
}

impl IntoElement for ComponentImage {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}

#[derive(PartialEq)]
struct ComponentPaint {
    image: Option<ImageHandle>,
    pixels: Option<ImageSource>,
    fit: ImageFit,
    inset: f32,
    radius: f32,
    owned: bool,
    scale: f32,
}

impl CanvasContent for ComponentPaint {
    fn draw(&self, context: &mut DrawingContext) {
        let r = context.bounds;
        context.fill(
            r,
            self.radius,
            Color::from_bgra(if self.owned {
                style::COMPONENT_READY
            } else {
                style::COMPONENT_EMPTY
            }),
        );
        if let Some(image) = &self.pixels {
            context.image(
                image,
                retinue_render::Bounds::new(
                    r.x + self.inset,
                    r.y + self.inset,
                    (r.width - self.inset * 2.0).max(0.0),
                    (r.height - self.inset * 2.0).max(0.0),
                ),
                self.fit,
            );
        }
        if self.owned {
            context.emit(style::component_glow(r, self.scale));
        }
    }
}
