use super::{Element, IntoElement, Kind, ViewContext};
use crate::{
    drawing::{CanvasContent, Drawing, DrawingContext, TextStyle, Typography},
    scene::Algorithm,
};
use retinue_render::{Bounds, CachePolicy};
use std::{any::Any, rc::Rc};

/// Resolve observed inputs into immutable layout and recording descriptions.
pub trait Primitive: PartialEq + 'static {
    fn resolve(&self, context: &mut ViewContext<'_>) -> PrimitiveOutput;
}

#[derive(Clone, PartialEq)]
pub struct PrimitiveOutput {
    pub style: taffy::Style,
    pub algorithm: Algorithm,
    pub drawing: Drawing,
    pub text_style: TextStyle,
    pub cache: CachePolicy,
}

impl PrimitiveOutput {
    pub fn new(content: impl CanvasContent, text_style: TextStyle) -> Self {
        Self {
            style: taffy::Style::default(),
            algorithm: Algorithm::Flex,
            drawing: Drawing::new(content),
            text_style,
            cache: CachePolicy::Never,
        }
    }

    pub fn fill_available(mut self) -> Self {
        self.style.size = taffy::Size {
            width: taffy::Dimension::percent(1.0),
            height: taffy::Dimension::percent(1.0),
        };
        self
    }
}

impl Element {
    pub fn primitive(content: impl Primitive) -> Self {
        Self::new(Kind::Primitive(Rc::new(content)))
    }
}

pub(crate) trait ErasedPrimitive {
    fn name(&self) -> &'static str;
    fn value(&self) -> &dyn Any;
    fn equals(&self, other: &dyn ErasedPrimitive) -> bool;
    fn resolve(&self, context: &mut ViewContext<'_>) -> PrimitiveOutput;
}

impl<P: Primitive> ErasedPrimitive for P {
    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }
    fn value(&self) -> &dyn Any {
        self
    }
    fn equals(&self, other: &dyn ErasedPrimitive) -> bool {
        other.value().downcast_ref::<P>() == Some(self)
    }
    fn resolve(&self, context: &mut ViewContext<'_>) -> PrimitiveOutput {
        Primitive::resolve(self, context)
    }
}

pub struct Canvas<C> {
    content: Rc<C>,
    cache: CachePolicy,
}

impl<C: CanvasContent> Canvas<C> {
    pub fn new(content: C) -> Self {
        Self {
            content: Rc::new(content),
            cache: CachePolicy::Never,
        }
    }

    pub fn cache_policy(mut self, cache: CachePolicy) -> Self {
        self.cache = cache;
        self
    }
}

impl<C: CanvasContent> PartialEq for Canvas<C> {
    fn eq(&self, other: &Self) -> bool {
        self.cache == other.cache
            && (Rc::ptr_eq(&self.content, &other.content) || self.content == other.content)
    }
}

struct Shared<C>(Rc<C>);
impl<C: CanvasContent> PartialEq for Shared<C> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<C: CanvasContent> CanvasContent for Shared<C> {
    fn draw(&self, context: &mut DrawingContext) {
        self.0.draw(context);
    }
}

impl<C: CanvasContent> Primitive for Canvas<C> {
    fn resolve(&self, context: &mut ViewContext<'_>) -> PrimitiveOutput {
        let mut output = PrimitiveOutput::new(
            Shared(self.content.clone()),
            context.environment().get::<Typography>(),
        )
        .fill_available();
        output.cache = self.cache;
        output
    }
}

impl<C: CanvasContent> IntoElement for Canvas<C> {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}

pub(crate) fn paint(
    output: &PrimitiveOutput,
    key: retinue_render::PaintKey,
) -> crate::scene::Paint {
    let drawing = output.drawing.clone();
    let text_style = output.text_style.clone();
    let mut paint = crate::scene::Paint::new(key, move |size| {
        drawing.record(
            Bounds::new(0.0, 0.0, size.width, size.height),
            text_style.clone(),
        )
    });
    paint.cache = output.cache;
    paint
}
