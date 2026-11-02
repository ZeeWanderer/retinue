use retinue_render::Bounds;
use retinue_ui::{layout::Available, prelude::*};

#[derive(PartialEq)]
pub(super) struct CardLayout;

impl Layout for CardLayout {
    type Cache = ();

    fn measure(
        &self,
        proposal: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        let value = |axis| match axis {
            Available::Limit(value) => value,
            _ => 0.0,
        };
        Ok(LayoutSize::new(
            value(proposal.width),
            value(proposal.height),
        ))
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        let weights = [1.05, 0.77, 0.8, 1.5];
        let mut top = bounds.y;
        for (i, weight) in weights.into_iter().enumerate() {
            let row = bounds.height * weight / 4.12;
            let height = match i {
                0 => 24.85,
                1 => 30.0,
                2 => 27.0,
                _ => row,
            };
            let width = if i == 2 {
                bounds.width * 0.8
            } else {
                bounds.width
            };
            children.place(
                i,
                Bounds::new(
                    bounds.x + (bounds.width - width) / 2.0,
                    top + (row - height) / 2.0,
                    width,
                    height,
                ),
            )?;
            top += row;
        }
        Ok(())
    }
}
