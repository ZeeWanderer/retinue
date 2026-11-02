use super::{Element, IntoElement, Primitive, PrimitiveOutput, ViewContext};
use crate::drawing::{AnimatedDrawing, CanvasContent, DrawingContext, Typography};
use std::rc::Rc;

pub struct AnimatedCanvas<C>(Rc<C>);

impl<C: AnimatedDrawing> AnimatedCanvas<C> {
    pub fn new(content: C) -> Self {
        Self(Rc::new(content))
    }
}
impl<C: AnimatedDrawing> PartialEq for AnimatedCanvas<C> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

struct Sample<C: AnimatedDrawing> {
    content: Rc<C>,
    data: C::Data,
}
impl<C: AnimatedDrawing> PartialEq for Sample<C> {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content && self.data == other.data
    }
}
impl<C: AnimatedDrawing> CanvasContent for Sample<C> {
    fn draw(&self, context: &mut DrawingContext) {
        self.content.draw(&self.data, context);
    }
}

impl<C: AnimatedDrawing> Primitive for AnimatedCanvas<C> {
    fn resolve(&self, context: &mut ViewContext<'_>) -> PrimitiveOutput {
        let data = context.animated("canvas", self.0.animatable_data());
        PrimitiveOutput::new(
            Sample {
                content: self.0.clone(),
                data,
            },
            context.environment().get::<Typography>(),
        )
        .fill_available()
    }
}

impl<C: AnimatedDrawing> IntoElement for AnimatedCanvas<C> {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}
