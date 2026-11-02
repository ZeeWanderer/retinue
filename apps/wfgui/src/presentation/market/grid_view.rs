use super::*;

#[derive(PartialEq)]
pub(super) struct GridView {
    pub model: Model,
    pub fonts: Fonts,
}

impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.model.metrics.get();
        let order = self.model.order.get();
        let grid = m.uniform_grid();
        let size = grid.extent(order.len()).expect("order extent");
        let cells = self
            .model
            .cells
            .order()
            .keys()
            .iter()
            .map(|&id| {
                card::OrderCardView {
                    id,
                    card: self.model.cells.item(id).unwrap(),
                    metrics: self.model.metrics.clone(),
                    fonts: self.fonts.clone(),
                }
                .id(id)
            })
            .collect::<Vec<_>>();
        let content = LayoutView::new(grid.arrange(order), cells).frame_aligned(
            size.width,
            size.height,
            retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING,
        );
        place(
            ScrollViewport::new(content).position(0.0, self.model.offset.get()),
            m.viewport(),
        )
        .id(GRID)
        .reuse(CacheHint::Conditional)
    }
}
