use super::DrawingContext;
use crate::motion::Animatable;

pub trait AnimatedDrawing: Animatable {
    fn draw(&self, data: &Self::Data, context: &mut DrawingContext);
}
