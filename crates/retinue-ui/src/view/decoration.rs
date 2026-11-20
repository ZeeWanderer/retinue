use crate::{
    Bounds,
    layout::{Layout, LayoutError, LayoutSize, Proposal, Subviews, anchors::AnchorPoint},
};

#[derive(PartialEq)]
pub(crate) struct Decoration {
    pub primary: usize,
    pub alignment: AnchorPoint,
}

impl Layout for Decoration {
    type Cache = ();
    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        children.measure(self.primary, proposal)
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        children.place(self.primary, bounds)?;
        let other = 1 - self.primary;
        place_overlay(other, self.alignment, bounds, children)
    }
}

fn place_overlay(
    other: usize,
    alignment: AnchorPoint,
    bounds: Bounds,
    children: &mut Subviews<'_, '_>,
) -> Result<(), LayoutError> {
    let size = children.measure(other, Proposal::bounded(bounds.width, bounds.height))?;
    children.place(
        other,
        Bounds::new(
            bounds.x + (bounds.width - size.width) * alignment.x,
            bounds.y + (bounds.height - size.height) * alignment.y,
            size.width,
            size.height,
        ),
    )
}
