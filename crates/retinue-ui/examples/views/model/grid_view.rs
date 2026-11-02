use super::{ITEM_COUNT, Model, card::Card, grid};
use retinue_ui::{prelude::*, view::ViewExt};

#[derive(PartialEq)]
pub(super) struct GridView {
    pub model: Model,
}

impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let (width, height) = self.model.size.get();
        let grid = grid(width);
        let offset = self.model.offset.get();
        let viewport_height = (height - 140.0).max(1.0);
        let visible = grid
            .visible(ITEM_COUNT, offset, viewport_height, 1)
            .unwrap();
        let content = LayoutView::new(
            grid.arrange(self.model.order.clone()),
            self.model.order.keys()[visible]
                .iter()
                .map(|key| {
                    Card {
                        number: key.value,
                        model: self.model.clone(),
                    }
                    .id(*key)
                })
                .collect::<Vec<_>>(),
        );
        ScrollViewport::new(content)
            .position(0.0, offset)
            .frame((width - 40.0).max(1.0), viewport_height)
    }
}
