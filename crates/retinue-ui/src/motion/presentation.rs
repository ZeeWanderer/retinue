use crate::{
    transition::{Interpolate, Tween},
    view::{Element, Kind, PrimitiveOutput, Rectangle},
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
    pub fn of(element: &Element, output: Option<&PrimitiveOutput>) -> Option<Self> {
        let (style, offset, rectangle) = match &*element.kind {
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

impl Interpolate for Color {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        let from = from.bgra();
        let to = to.bgra();
        let alpha =
            f32::interpolate(&(from[3] as f32), &(to[3] as f32), progress).clamp(0.0, 255.0);
        let mut value = [0; 4];
        for i in 0..3 {
            let color = f32::interpolate(
                &(from[i] as f32 * from[3] as f32),
                &(to[i] as f32 * to[3] as f32),
                progress,
            );
            value[i] = if alpha > 0.0 {
                (color / alpha).round().clamp(0.0, 255.0) as u8
            } else {
                0
            };
        }
        value[3] = alpha.round() as u8;
        Color::from_bgra(value)
    }
}

pub(crate) struct Presentation {
    pub tween: Tween<Visual>,
    pub value: Visual,
}
