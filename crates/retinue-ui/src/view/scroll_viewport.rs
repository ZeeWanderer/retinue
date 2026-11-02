use super::{Element, IntoElement, Kind, LayoutDescription, LayoutKind, properties::Properties};
use crate::{
    Bounds,
    layout::{
        Available, Insets, Layout, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews,
    },
    scene::Algorithm,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxes {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

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
        Element::new(Kind::Layout(LayoutDescription {
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
        }))
    }
}

#[derive(PartialEq)]
struct ScrollLayout {
    axes: ScrollAxes,
    insets: Insets,
}

impl ScrollLayout {
    fn proposal(&self, viewport: Proposal) -> Proposal {
        let inset = |value: Available, amount: f32| match value {
            Available::Limit(value) => Available::Limit((value - amount).max(0.0)),
            other => other,
        };
        Proposal {
            width: if self.axes == ScrollAxes::Vertical {
                inset(viewport.width, self.insets.left + self.insets.right)
            } else {
                Available::Ideal
            },
            height: if self.axes == ScrollAxes::Horizontal {
                inset(viewport.height, self.insets.top + self.insets.bottom)
            } else {
                Available::Ideal
            },
        }
    }
}

impl Layout for ScrollLayout {
    type Cache = ();
    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        let content = children.measure(0, self.proposal(proposal))?;
        let resolve = |available, ideal| match available {
            Available::Limit(value) => value,
            _ => ideal,
        };
        Ok(LayoutSize::new(
            resolve(
                proposal.width,
                content.width + self.insets.left + self.insets.right,
            ),
            resolve(
                proposal.height,
                content.height + self.insets.top + self.insets.bottom,
            ),
        ))
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        let content = children.measure(
            0,
            self.proposal(Proposal::bounded(bounds.width, bounds.height)),
        )?;
        let width = if self.axes == ScrollAxes::Vertical {
            (bounds.width - self.insets.left - self.insets.right).max(0.0)
        } else {
            content.width
        };
        let height = if self.axes == ScrollAxes::Horizontal {
            (bounds.height - self.insets.top - self.insets.bottom).max(0.0)
        } else {
            content.height
        };
        children.place(
            0,
            Bounds::new(
                bounds.x + self.insets.left,
                bounds.y + self.insets.top,
                width,
                height,
            ),
        )
    }
}
