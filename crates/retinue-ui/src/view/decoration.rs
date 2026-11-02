use crate::{
    Bounds,
    layout::{Layout, LayoutError, LayoutSize, Proposal, Subviews, anchors::AnchorPoint},
};

#[derive(PartialEq)]
pub(super) struct Decoration {
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
        let size = children.measure(other, Proposal::bounded(bounds.width, bounds.height))?;
        children.place(
            other,
            Bounds::new(
                bounds.x + (bounds.width - size.width) * self.alignment.x,
                bounds.y + (bounds.height - size.height) * self.alignment.y,
                size.width,
                size.height,
            ),
        )
    }
}
