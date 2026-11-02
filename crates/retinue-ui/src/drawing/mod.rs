//! Local-coordinate recording shared by primitive views and custom content.

mod animated;
mod commands;
mod context;
mod layers;
mod pixels;
pub use animated::AnimatedDrawing;
pub use commands::{DrawList, ExternalDrawing};
pub use layers::{LayeredDrawing, PixelLayers};
pub use pixels::PixelCanvas;
pub mod text;

pub use context::DrawingContext;
pub use text::{TextStyle, Typography};

use std::{any::Any, rc::Rc};

/// Comparable drawing inputs. Read observed state when constructing these inputs,
/// not during replay; recording can run more than once or be skipped entirely.
pub trait CanvasContent: PartialEq + 'static {
    fn draw(&self, context: &mut DrawingContext);
}

#[derive(Clone)]
pub struct Drawing(Rc<dyn ErasedContent>);

impl Drawing {
    pub fn new(content: impl CanvasContent) -> Self {
        Self(Rc::new(content))
    }

    pub fn record(&self, bounds: crate::Bounds, text: TextStyle) -> Vec<retinue_render::Command> {
        let mut context = DrawingContext::new(bounds, text);
        self.0.draw(&mut context);
        context.finish()
    }
}

impl PartialEq for Drawing {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0.equals(&*other.0)
    }
}

trait ErasedContent {
    fn value(&self) -> &dyn Any;
    fn equals(&self, other: &dyn ErasedContent) -> bool;
    fn draw(&self, context: &mut DrawingContext);
}

impl<C: CanvasContent> ErasedContent for C {
    fn value(&self) -> &dyn Any {
        self
    }
    fn equals(&self, other: &dyn ErasedContent) -> bool {
        other.value().downcast_ref::<C>() == Some(self)
    }
    fn draw(&self, context: &mut DrawingContext) {
        CanvasContent::draw(self, context);
    }
}
