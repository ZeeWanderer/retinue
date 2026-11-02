use super::{Model, grid_view::GridView, tab::Tab};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(super) struct Root(pub Model);

impl View for Root {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let (width, height) = self.0.size.get();
        ui! {
            VStack(16.0) {
                HStack(8.0) {
                    for (index, title) in ["Warframes", "Primary", "Secondary", "Melee"].into_iter().enumerate() {
                        Tab { selected: self.0.selected.clone(), number: 10 + index as u64, title }.id(10 + index as u64)
                    }
                }.id(2u64)
                Text("Retained views, keyed cards and streamed images").font_size(18.0)
                GridView { model: self.0.clone() }.id(6u64)
            }.alignment(Alignment::Start).padding(20.0).frame(width, height).id(1u64)
        }
    }
}
