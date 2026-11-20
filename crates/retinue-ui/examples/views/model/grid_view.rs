use super::{ITEM_COUNT, Model, card::Card, grid};
use retinue_ui::authoring::*;

#[derive(PartialEq)]
pub(super) struct GridView {
    pub model: Model,
}

#[view]
impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let (width, height) = self.model.size.get();
        let grid = grid(width);
        let offset = self.model.offset.get();
        let viewport_height = (height - 140.0).max(1.0);
        let visible = grid
            .visible(ITEM_COUNT, offset, viewport_height, 1)
            .unwrap();
        let content = ui! {
            LayoutView(grid.arrange(self.model.order.clone())) {
                ForEach(self.model.order.keys()[visible].iter().copied(), |key| *key) { |key|
                    compose(Card { number: key.value, model: self.model.clone() }).named(key)
                }
            }
        };
        ScrollViewport(content).position(0.0, offset).frame(
            Frame::new()
                .width((width - 40.0).max(1.0))
                .height(viewport_height),
        )
    }
}
