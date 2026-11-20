#[cfg(any(test, feature = "reference-frontend"))]
use super::{Element, IntoElement};
use super::{Primitive, PrimitiveOutput, ViewContext};
use crate::{
    Bounds,
    drawing::{
        CanvasContent, DrawingContext, Typography,
        text::{ParagraphDrawing, ParagraphSpec, ParagraphState, TextAlignment},
    },
    layout::{
        Available, Layout, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews,
        alignment::{AlignmentGuide, AlignmentId, Axis},
    },
    scene::Algorithm,
};
use retinue_render::{Color, painter::CachedFont};
use std::{rc::Rc, sync::Arc};

#[derive(Clone)]
pub struct Text {
    text: Arc<str>,
    font: Option<Rc<CachedFont>>,
    size: Option<f32>,
    color: Option<Color>,
    alignment: TextAlignment,
    line_limit: Option<usize>,
}

impl Text {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self {
            text: text.into(),
            font: None,
            size: None,
            color: None,
            alignment: TextAlignment::Leading,
            line_limit: None,
        }
    }
    pub fn font(mut self, font: Rc<CachedFont>) -> Self {
        self.font = Some(font);
        self
    }
    pub fn font_size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn alignment(mut self, alignment: TextAlignment) -> Self {
        self.alignment = alignment;
        self
    }
    pub fn line_limit(mut self, limit: usize) -> Self {
        self.line_limit = Some(limit);
        self
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.font.as_ref().map(|font| font.id()) == other.font.as_ref().map(|font| font.id())
            && self.size == other.size
            && self.color == other.color
            && self.alignment == other.alignment
            && self.line_limit == other.line_limit
    }
}

impl Primitive for Text {
    fn resolve(&self, context: &mut ViewContext<'_>) -> PrimitiveOutput {
        self.resolve_with(context.environment().get::<Typography>())
    }
}

impl Text {
    pub(crate) fn resolve_with(&self, mut style: crate::drawing::TextStyle) -> PrimitiveOutput {
        if let Some(font) = &self.font {
            style.font = font.clone();
        }
        if let Some(size) = self.size {
            style.size = size;
        }
        if let Some(color) = self.color {
            style.color = color;
        }
        let layout = TextMeasure {
            paragraph: ParagraphSpec::new(self.text.clone(), &style, self.line_limit),
            alignment: self.alignment,
        };
        let mut output = PrimitiveOutput::new(
            TextPaint {
                layout: layout.clone(),
                color: style.color,
            },
            style,
        );
        output.algorithm = Algorithm::Custom(LayoutAlgorithm::new(layout));
        output
    }
}

#[cfg(any(test, feature = "reference-frontend"))]
impl IntoElement for Text {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}

pub struct FirstTextBaseline;
impl AlignmentId for FirstTextBaseline {
    const AXIS: Axis = Axis::Vertical;
    fn default_value(size: LayoutSize) -> f32 {
        size.height
    }
}
pub struct LastTextBaseline;
impl AlignmentId for LastTextBaseline {
    const AXIS: Axis = Axis::Vertical;
    fn default_value(size: LayoutSize) -> f32 {
        size.height
    }
}

#[derive(PartialEq)]
struct TextPaint {
    layout: TextMeasure,
    color: Color,
}
impl CanvasContent for TextPaint {
    fn draw(&self, context: &mut DrawingContext) {
        context
            .layout::<TextMeasure>()
            .and_then(|cache| cache.placed.as_ref())
            .expect("text placement validated before recording")
            .draw(context, context.bounds, self.color);
    }
}

#[derive(Clone, PartialEq)]
struct TextMeasure {
    paragraph: ParagraphSpec,
    alignment: TextAlignment,
}

#[derive(Default)]
struct TextCache {
    paragraph: ParagraphState,
    placed: Option<ParagraphDrawing>,
}

impl Layout for TextMeasure {
    type Cache = TextCache;
    fn measure(
        &self,
        proposal: Proposal,
        _: &mut Subviews<'_, '_>,
        cache: &mut TextCache,
    ) -> Result<LayoutSize, LayoutError> {
        let width = match proposal.width {
            Available::Ideal => cache
                .paragraph
                .ideal_width(&self.paragraph)
                .map_err(LayoutError::Text)?,
            Available::MinContent => cache
                .paragraph
                .minimum_width(&self.paragraph)
                .map_err(LayoutError::Text)?,
            Available::Limit(width) => width,
        };
        Ok(cache
            .paragraph
            .layout(&self.paragraph, width, self.alignment)
            .map_err(LayoutError::Text)?
            .size)
    }
    fn place(
        &self,
        bounds: Bounds,
        _: &mut Subviews<'_, '_>,
        cache: &mut TextCache,
    ) -> Result<(), LayoutError> {
        cache.placed = Some(
            cache
                .paragraph
                .layout(&self.paragraph, bounds.width, self.alignment)
                .map_err(LayoutError::Text)?
                .drawing(),
        );
        Ok(())
    }
    fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        _: &mut Subviews<'_, '_>,
        cache: &mut TextCache,
    ) -> Result<Option<f32>, LayoutError> {
        let layout = cache
            .paragraph
            .layout(&self.paragraph, bounds.width, self.alignment)
            .map_err(LayoutError::Text)?;
        if guide.is::<FirstTextBaseline>() {
            Ok(Some(bounds.y + layout.baseline))
        } else if guide.is::<LastTextBaseline>() {
            Ok(Some(bounds.y + layout.last_baseline))
        } else {
            Ok(None)
        }
    }
}
