use super::Alignment;
use crate::{
    Bounds,
    layout::{Layout, LayoutError, LayoutSize, Proposal, Subviews},
};

#[derive(PartialEq)]
pub(crate) struct StackLayout {
    pub alignment: Alignment,
}

impl Layout for StackLayout {
    type Cache = ();

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
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
        let align = match self.alignment {
            Alignment::Start | Alignment::Stretch => 0.0,
            Alignment::Center => 0.5,
            Alignment::End => 1.0,
        };
        for child in 0..children.len() {
            let size = children.measure(child, Proposal::bounded(bounds.width, bounds.height))?;
            children.place(
                child,
                Bounds::new(
                    bounds.x + (bounds.width - size.width) * align,
                    bounds.y + (bounds.height - size.height) * align,
                    size.width,
                    size.height,
                ),
            )?;
        }
        Ok(())
    }
}
