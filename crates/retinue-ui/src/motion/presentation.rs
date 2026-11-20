use crate::{
    runtime::instance::{Input, Kind},
    transition::{Interpolate, Tween},
    view::{PrimitiveOutput, Rectangle},
};
use retinue_render::Color;
use taffy::Style;

#[derive(Clone, PartialEq)]
pub(crate) struct Visual {
    pub style: Option<Style>,
    pub offset: (f32, f32),
    pub rectangle: Option<Rectangle>,
}

impl Visual {
    pub fn of(input: &Input, output: Option<&PrimitiveOutput>) -> Option<Self> {
        let (style, offset, rectangle) = match &input.kind {
            Kind::Layout(layout) => (Some(layout.style.clone()), layout.properties.offset, None),
            Kind::Primitive(_) => (Some(output?.style.clone()), (0.0, 0.0), None),
            Kind::Rectangle(rectangle) => (None, (0.0, 0.0), Some(*rectangle)),
            _ => return None,
        };
        Some(Self {
            style,
            offset,
            rectangle,
        })
    }
}

impl Interpolate for Visual {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        let style = match (&from.style, &to.style) {
            (Some(from), Some(to)) => Some(super::style::interpolate(from, to, progress)),
            _ => to.style.clone(),
        };
        let rectangle = match (from.rectangle, to.rectangle) {
            (Some(from), Some(to)) => Some(Rectangle {
                color: Color::interpolate(&from.color, &to.color, progress),
                radius: f32::interpolate(&from.radius, &to.radius, progress).max(0.0),
            }),
            _ => to.rectangle,
        };
        Self {
            style,
            rectangle,
            offset: <(f32, f32)>::interpolate(&from.offset, &to.offset, progress),
        }
    }
}

pub(crate) struct Presentation {
    pub tween: Tween<Visual>,
    pub value: Visual,
}
