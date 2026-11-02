use super::{Layout, LayoutError, LayoutSize, Proposal, Subviews, values::LayoutValueKey};
use crate::Bounds;

/// Child-local frames for a layout whose geometry is already known.
pub struct Placement;

impl LayoutValueKey for Placement {
    type Value = Option<Bounds>;
    fn default_value() -> Self::Value {
        None
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Positioned {
    size: LayoutSize,
}

impl Positioned {
    pub fn new(size: LayoutSize) -> Self {
        Self { size }
    }
}

impl Layout for Positioned {
    type Cache = ();

    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(self.size)
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        for index in 0..children.len() {
            let local = children.value::<Placement>(index)?.unwrap_or(Bounds::new(
                0.0,
                0.0,
                bounds.width,
                bounds.height,
            ));
            children.place(
                index,
                Bounds::new(
                    bounds.x + local.x,
                    bounds.y + local.y,
                    local.width,
                    local.height,
                ),
            )?;
        }
        Ok(())
    }
}
