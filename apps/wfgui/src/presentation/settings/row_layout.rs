use retinue_ui::{
    Bounds, layout::Available, layout::Layout, layout::LayoutError, layout::LayoutSize,
    layout::Proposal, layout::Subviews,
};

#[derive(PartialEq)]
pub(super) struct RowLayout {
    pub scale: f32,
}

impl RowLayout {
    fn labels(&self, width: f32) -> Proposal {
        Proposal {
            width: Available::Limit((width - 136.0 * self.scale).max(0.0)),
            height: Available::Ideal,
        }
    }
}

impl Layout for RowLayout {
    type Cache = ();

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        let width = match proposal.width {
            Available::Limit(width) => width,
            width => {
                children
                    .measure(
                        0,
                        Proposal {
                            width,
                            height: Available::Ideal,
                        },
                    )?
                    .width
                    + 136.0 * self.scale
            }
        };
        let labels = children.measure(0, self.labels(width))?;
        let button = children.measure(1, Proposal::IDEAL)?;
        Ok(LayoutSize::new(
            width,
            labels.height.max(button.height) + 32.0 * self.scale,
        ))
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        let labels = children.measure(0, self.labels(bounds.width))?;
        let button = children.measure(1, Proposal::IDEAL)?;
        let padding = 16.0 * self.scale;
        children.place(
            0,
            Bounds::new(
                bounds.x + padding,
                bounds.y + (bounds.height - labels.height) / 2.0,
                (bounds.width - 136.0 * self.scale).max(0.0),
                labels.height,
            ),
        )?;
        children.place(
            1,
            Bounds::new(
                bounds.x + bounds.width - padding - button.width,
                bounds.y + (bounds.height - button.height) / 2.0,
                button.width,
                button.height,
            ),
        )
    }
}
