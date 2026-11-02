use super::*;

#[derive(PartialEq)]
pub(super) struct GridView {
    pub model: Model,
    pub fonts: Fonts,
}

impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let model = &self.model;
        let m = model.metrics.get();
        let order = model.order.get();
        let grid = m.uniform_grid();
        let extent = grid.extent(order.len()).expect("listing extent");
        let cells = model
            .cells
            .order()
            .keys()
            .iter()
            .map(|&id| {
                let cell = model.cells.item(id).expect("visible cell");
                let card = match m.page {
                    Page::Inventory => inventory::InventoryCardView {
                        id,
                        cell,
                        metrics: model.metrics.clone(),
                        fonts: self.fonts.clone(),
                    }
                    .into_element(),
                    Page::Mastery => mastery::MasteryCardView {
                        id,
                        cell,
                        metrics: model.metrics.clone(),
                        context: model.context.clone(),
                        fonts: self.fonts.clone(),
                    }
                    .into_element(),
                    Page::Relics => relics::RelicCardView {
                        id,
                        cell,
                        metrics: model.metrics.clone(),
                        context: model.context.clone(),
                        fonts: self.fonts.clone(),
                    }
                    .into_element(),
                    _ => unreachable!(),
                };
                card.id(id)
            })
            .collect::<Vec<_>>();
        let content = LayoutView::new(grid.arrange(order), cells)
            .frame_aligned(
                extent.width,
                extent.height,
                retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING,
            )
            .id(CONTENT);
        place(
            ScrollViewport::new(content).position(0.0, model.offset.get()),
            m.viewport(),
        )
        .id(VIEWPORT)
    }
}
