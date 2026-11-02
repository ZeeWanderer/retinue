use super::{CanvasContent, DrawingContext};
use retinue_render::Command;
use std::rc::Rc;

#[derive(Clone, Default)]
pub struct DrawList(pub Rc<Vec<Command>>);

impl PartialEq for DrawList {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl From<Vec<Command>> for DrawList {
    fn from(commands: Vec<Command>) -> Self {
        Self(Rc::new(commands))
    }
}

impl CanvasContent for DrawList {
    fn draw(&self, context: &mut DrawingContext) {
        context.emit(Command::Translate {
            offset: (0.0, 0.0),
            commands: self.0.clone(),
        });
    }
}

/// A host-owned draw slot. Its damage and lifecycle are supplied by that host.
#[derive(Clone, PartialEq)]
pub struct ExternalDrawing(pub crate::Key);
impl CanvasContent for ExternalDrawing {
    fn draw(&self, context: &mut DrawingContext) {
        context.emit(Command::External {
            key: self.0,
            bounds: context.bounds,
        });
    }
}
