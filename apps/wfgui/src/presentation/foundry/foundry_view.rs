use super::{
    composition::GRID, composition::OVERLAYS, composition::TOOLBAR, filters_view::FiltersView,
    foundry_model::Model, grid_view::GridView, overlays_view::OverlaysView,
    toolbar_view::ToolbarView,
};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub struct FoundryView(pub(super) Model);

impl View for FoundryView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let area = self.0.area.get();
        ui! {
            ZStack() {
                ToolbarView { model: self.0.clone() }.id(TOOLBAR).hit_testing(!self.0.modal.get())
                GridView { model: self.0.clone() }.id(GRID).hit_testing(!self.0.modal.get())
                OverlaysView { model: self.0.clone() }.id(OVERLAYS)
                FiltersView { model: self.0.clone() }
            }.alignment(Alignment::Start).frame(area.width, area.height)
        }
    }
}
