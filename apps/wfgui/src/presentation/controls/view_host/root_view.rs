use super::{CHROME, OVERLAYS, PaintSlot};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(super) struct RootView(pub Element);

impl View for RootView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            ZStack() {
                self.0.clone()
                PaintSlot(CHROME)
                PaintSlot(OVERLAYS)
            }.alignment(Alignment::Start)
        }
    }
}
