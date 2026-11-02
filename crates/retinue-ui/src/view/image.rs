use super::{Element, IntoElement, Primitive, PrimitiveOutput, ViewContext};
use crate::{
    Bounds,
    drawing::{CanvasContent, DrawingContext, TextStyle},
    image::ImageResource,
    layout::{Layout, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews},
    scene::Algorithm,
};
use retinue_render::{ImageFit, ImageSource};

#[derive(Clone, PartialEq)]
pub struct Image {
    resource: ImageResource,
    fit: ImageFit,
    resizable: bool,
    placeholder_size: LayoutSize,
}

impl Image {
    pub fn new(resource: ImageResource) -> Self {
        Self {
            resource,
            fit: ImageFit::Contain,
            resizable: false,
            placeholder_size: LayoutSize::ZERO,
        }
    }
    pub fn resizable(mut self) -> Self {
        self.resizable = true;
        self
    }
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }
    pub fn placeholder_size(mut self, width: f32, height: f32) -> Self {
        self.placeholder_size = LayoutSize::new(width, height);
        self
    }
}

impl Primitive for Image {
    fn resolve(&self, _: &mut ViewContext<'_>) -> PrimitiveOutput {
        let mut output = PrimitiveOutput::new(
            ImagePaint {
                pixels: self.resource.pixels(),
                fit: self.fit,
            },
            TextStyle::default(),
        );
        if self.resizable {
            output = output.fill_available();
        } else {
            let size = self.resource.metadata().unwrap_or(self.placeholder_size);
            output.algorithm = Algorithm::Custom(LayoutAlgorithm::new(ImageMeasure(size)));
            output.style.flex_shrink = 0.0;
        }
        output
    }
}

impl IntoElement for Image {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}

#[derive(PartialEq)]
struct ImagePaint {
    pixels: Option<ImageSource>,
    fit: ImageFit,
}
impl CanvasContent for ImagePaint {
    fn draw(&self, context: &mut DrawingContext) {
        if let Some(image) = &self.pixels {
            context.image(image, context.bounds, self.fit);
        }
    }
}

#[derive(PartialEq)]
struct ImageMeasure(LayoutSize);
impl Layout for ImageMeasure {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(self.0)
    }
    fn place(&self, _: Bounds, _: &mut Subviews<'_, '_>, _: &mut ()) -> Result<(), LayoutError> {
        Ok(())
    }
}
