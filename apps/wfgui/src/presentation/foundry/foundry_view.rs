use super::{
    composition::GRID, composition::OVERLAYS, composition::TOOLBAR, filters_view::FiltersView,
    foundry_model::Model, grid_view::GridView, overlays_view::OverlaysView,
    toolbar_view::ToolbarView,
};
use crate::presentation::widgets::compiled::frame;
use retinue_ui::{authoring::*, layout::anchors::AnchorPoint};

#[derive(PartialEq)]
pub struct FoundryView(pub(super) Model);

#[retinue_ui::authoring::view]
impl View for FoundryView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let area = self.0.area.get();
        ui! {
            ZStack() {
                Canvas(self.0.background_paint.get())
                compose(ToolbarView { model: self.0.clone() }).named(TOOLBAR).hit_testing(!self.0.modal.get())
                compose(GridView { model: self.0.clone() }).named(GRID).hit_testing(!self.0.modal.get())
                compose(OverlaysView { model: self.0.clone() }).named(OVERLAYS)
                compose(FiltersView { model: self.0.clone() })
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(area.width, area.height))
        }
    }
}
