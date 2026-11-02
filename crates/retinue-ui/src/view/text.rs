use super::{Element, IntoElement, Primitive, PrimitiveOutput, ViewContext};
use crate::{
    Bounds,
    drawing::{
        CanvasContent, DrawingContext, Typography,
        text::{Paragraph, TextAlignment},
    },
    layout::{
        Available, Layout, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews,
        alignment::{AlignmentGuide, AlignmentId, Axis},
    },
    scene::Algorithm,
};
use retinue_render::{Color, painter::CachedFont};
use std::rc::Rc;

#[derive(Clone)]
pub struct Text {
    text: Rc<str>,
    font: Option<Rc<CachedFont>>,
    size: Option<f32>,
    color: Option<Color>,
    alignment: TextAlignment,
    line_limit: Option<usize>,
}

impl Text {
    pub fn new(text: impl Into<Rc<str>>) -> Self {
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
        let mut style = context.environment().get::<Typography>();
        if let Some(font) = &self.font {
            style.font = font.clone();
        }
        if let Some(size) = self.size {
            style.size = size;
        }
        if let Some(color) = self.color {
            style.color = color;
        }
        let paragraph = Rc::new(Paragraph::new(self.text.clone(), &style, self.line_limit));
        let mut output = PrimitiveOutput::new(
            TextPaint {
                paragraph: paragraph.clone(),
                color: style.color,
                alignment: self.alignment,
            },
            style,
        );
        output.algorithm = Algorithm::Custom(LayoutAlgorithm::new(TextMeasure(paragraph)));
        output
    }
}

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
    paragraph: Rc<Paragraph>,
    color: Color,
    alignment: TextAlignment,
}
impl CanvasContent for TextPaint {
    fn draw(&self, context: &mut DrawingContext) {
        self.paragraph.layout(context.bounds.width).draw(
            context,
            context.bounds,
            self.color,
            self.alignment,
        );
    }
}

#[derive(PartialEq)]
struct TextMeasure(Rc<Paragraph>);
impl Layout for TextMeasure {
    type Cache = ();
    fn measure(
        &self,
        proposal: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        let width = match proposal.width {
            Available::Ideal => self.0.ideal_width(),
            Available::MinContent => self.0.minimum_width(),
            Available::Limit(width) => width,
        };
        Ok(self.0.layout(width).size)
    }
    fn place(&self, _: Bounds, _: &mut Subviews<'_, '_>, _: &mut ()) -> Result<(), LayoutError> {
        Ok(())
    }
    fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<Option<f32>, LayoutError> {
        let layout = self.0.layout(bounds.width);
        if guide.is::<FirstTextBaseline>() {
            Ok(Some(bounds.y + layout.baseline))
        } else if guide.is::<LastTextBaseline>() {
            Ok(Some(
                bounds.y
                    + layout.baseline
                    + (layout.lines.len().saturating_sub(1) as f32) * self.0.line_height,
            ))
        } else {
            Ok(None)
        }
    }
}
