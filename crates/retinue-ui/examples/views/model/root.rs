use super::{Model, grid_view::GridView, tab::Tab};
use retinue_ui::authoring::*;

#[derive(PartialEq)]
pub(super) struct Root {
    pub model: Model,
}

#[view]
impl View for Root {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let (width, height) = self.model.size.get();
        ui! {
            VStack(16.0) {
                HStack(8.0) {
                    ForEach(["Warframes", "Primary", "Secondary", "Melee"].into_iter().enumerate(), |(i, _)| 10 + *i as u64) { |(index, title)|
                        let number = 10 + index as u64;
                        compose(Tab { selected: self.model.selected.clone(), number, title }).named(number.into())
                    }
                }.named(2.into())
                Text("Retained views, keyed cards and streamed images").font_size(18.0)
                compose(GridView { model: self.model.clone() }).named(6.into())
            }.alignment(HorizontalAlignment::Leading).padding(20.0).frame(Frame::new().width(width).height(height)).named(1.into())
        }
    }
}
