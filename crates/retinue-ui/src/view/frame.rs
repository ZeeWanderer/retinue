use crate::{
    Bounds,
    layout::{Layout, LayoutError, LayoutSize, Proposal, Subviews, anchors::AnchorPoint},
};

#[derive(PartialEq)]
pub(super) struct FrameLayout {
    pub alignment: AnchorPoint,
}

impl Layout for FrameLayout {
    type Cache = ();

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        children.measure(0, proposal)
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        let size = children.measure(0, Proposal::bounded(bounds.width, bounds.height))?;
        children.place(
            0,
            Bounds::new(
                bounds.x + (bounds.width - size.width) * self.alignment.x,
                bounds.y + (bounds.height - size.height) * self.alignment.y,
                size.width,
                size.height,
            ),
        )
    }
}
