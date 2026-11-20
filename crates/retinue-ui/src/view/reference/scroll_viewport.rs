use super::{Element, IntoElement, LayoutDescription, LayoutKind, properties::Properties};
pub use crate::layout::scroll::ScrollAxes;
use crate::{
    layout::{Insets, LayoutAlgorithm, scroll::ScrollLayout},
    scene::Algorithm,
};

/// Clipped content space. The input/controller layer owns the scroll position.
pub struct ScrollViewport {
    content: Element,
    axes: ScrollAxes,
    position: (f32, f32),
    insets: Insets,
}

impl ScrollViewport {
    pub fn new(content: impl IntoElement) -> Self {
        Self {
            content: content.into_element(),
            axes: ScrollAxes::Vertical,
            position: (0.0, 0.0),
            insets: Insets::default(),
        }
    }
    pub fn axes(mut self, axes: ScrollAxes) -> Self {
        self.axes = axes;
        self
    }
    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.position = (x, y);
        self
    }
    pub fn content_insets(mut self, insets: impl Into<Insets>) -> Self {
        self.insets = insets.into();
        self
    }
}

impl IntoElement for ScrollViewport {
    fn into_element(self) -> Element {
        use super::ViewExt;
        let offset = match self.axes {
            ScrollAxes::Vertical => (0.0, -self.position.1),
            ScrollAxes::Horizontal => (-self.position.0, 0.0),
            ScrollAxes::Both => (-self.position.0, -self.position.1),
        };
        Element::layout(LayoutDescription {
            kind: LayoutKind::ScrollViewport,
            style: taffy::Style::default(),
            algorithm: Algorithm::Custom(LayoutAlgorithm::new(ScrollLayout {
                axes: self.axes,
                insets: self.insets,
            })),
            children: vec![self.content.offset(offset.0, offset.1)],
            properties: Properties {
                clip: true,
                ..Properties::default()
            },
        })
    }
}
