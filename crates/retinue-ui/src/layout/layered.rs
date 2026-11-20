use super::{Layout, LayoutError, LayoutSize, Proposal, Subviews, anchors::AnchorPoint};
use crate::Bounds;

#[derive(PartialEq)]
pub(crate) struct Layered {
    pub alignment: AnchorPoint,
}

impl Layered {
    fn place_child(
        &self,
        child: usize,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError> {
        let size = children.measure(child, Proposal::bounded(bounds.width, bounds.height))?;
        children.place(
            child,
            Bounds::new(
                bounds.x + (bounds.width - size.width) * self.alignment.x,
                bounds.y + (bounds.height - size.height) * self.alignment.y,
                size.width,
                size.height,
            ),
        )
    }
}

impl Layout for Layered {
    type Cache = ();

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        if children.len() == 1 {
            return children.measure(0, proposal);
        }
        let mut size = LayoutSize::ZERO;
        for child in 0..children.len() {
            let measured = children.measure(child, proposal)?;
            size.width = size.width.max(measured.width);
            size.height = size.height.max(measured.height);
        }
        Ok(size)
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        if children.len() == 1 {
            return self.place_child(0, bounds, children);
        }
        for child in 0..children.len() {
            self.place_child(child, bounds, children)?;
        }
        Ok(())
    }
}
