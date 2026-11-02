use retinue_ui::{
    Bounds, layout::Layout, layout::LayoutError, layout::LayoutSize, layout::Proposal,
    layout::Subviews,
};

#[derive(Clone, PartialEq)]
pub struct DockLayout {
    pub width: f32,
    pub height: f32,
    pub inset: f32,
    pub left: f32,
    pub left_gap: f32,
    pub right: f32,
    pub right_gap: f32,
    pub docked: bool,
}

impl Layout for DockLayout {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(LayoutSize::new(self.width, self.height))
    }
    fn place(
        &self,
        b: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        let center_x = self.inset + self.left + self.left_gap;
        let right_x = (b.width - self.inset - self.right).max(center_x);
        let center_end = if self.docked {
            right_x - self.right_gap
        } else {
            b.width - self.inset
        };
        children.place(0, Bounds::new(b.x + self.inset, b.y, self.left, b.height))?;
        children.place(
            1,
            Bounds::new(
                b.x + center_x,
                b.y,
                (center_end - center_x).max(0.0),
                b.height,
            ),
        )?;
        children.place(2, Bounds::new(b.x + right_x, b.y, self.right, b.height))
    }
}
