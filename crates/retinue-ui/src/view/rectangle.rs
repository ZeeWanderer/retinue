use super::{Element, IntoElement, Kind};
use retinue_render::Color;

#[derive(Clone, Copy, PartialEq)]
pub struct Rectangle {
    pub color: Color,
    pub radius: f32,
}

impl Rectangle {
    pub fn new(color: Color) -> Self {
        Self { color, radius: 0.0 }
    }
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl IntoElement for Rectangle {
    fn into_element(self) -> Element {
        Element::new(Kind::Rectangle(self))
    }
}
