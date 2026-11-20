use super::{card_view::CardView, composition::CONTENT, foundry_model::Model};
use crate::presentation::widgets::compiled::frame;
use retinue_ui::authoring::*;

#[derive(PartialEq)]
pub(super) struct GridContentView {
    pub model: Model,
}

#[retinue_ui::authoring::view]
impl View for GridContentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let model = &self.model;
        #[cfg(test)]
        model.grid_builds.set(model.grid_builds.get() + 1);
        let extent = model.extent.get();
        ui! {
            LayoutView::new(model.grid.get()) {
                ItemViews::<CardView>(model.cards.clone())
            }.frame(frame(extent.0, extent.1)).named(CONTENT)
        }
    }
}
