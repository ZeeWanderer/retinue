use super::{Available, Insets, Layout, LayoutError, LayoutSize, Proposal, Subviews};
use crate::Bounds;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxes {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

#[derive(PartialEq)]
pub(crate) struct ScrollLayout {
    pub axes: ScrollAxes,
    pub insets: Insets,
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
