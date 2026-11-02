use super::{card_view::CardView, composition::CONTENT, foundry_model::Model};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub struct GridView {
    pub model: Model,
}
impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let model = &self.model;
        let viewport = model.viewport.get();
        let offset = model.offset.get();
        let extent = model.extent.get();
        let order = model.cards.order();
        let content = LayoutView::new(
            model.grid.get(),
            order
                .keys()
                .iter()
                .map(|&key| {
                    CardView {
                        key,
                        content: model.cards.item(key).unwrap(),
                    }
                    .id(key)
                })
                .collect::<Vec<_>>(),
        )
        .frame_aligned(
            extent.0,
            extent.1,
            retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING,
        )
        .id(CONTENT);
        ScrollViewport::new(content)
            .axes(ScrollAxes::Both)
            .position(-offset.0, -offset.1)
            .frame(viewport.width, viewport.height)
            .offset(viewport.x, viewport.y)
    }
}
