use super::{CARD, GREEN, MUTED, Model};
use retinue_render::CacheHint;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(super) struct Card {
    pub number: u64,
    pub model: Model,
}

impl View for Card {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let resource = self.model.images[usize::from(self.model.replacement.get())].clone();
        ui! {
            HStack(12.0) {
                Image(resource).resizable().frame(64.0, 64.0)
                VStack(5.0) {
                    Text(format!("Equipment {}", self.number)).font_size(16.0)
                    Text("Blueprint").color(MUTED)
                    Text(format!("Owned: {}", self.model.counter.get())).color(GREEN)
                }.alignment(Alignment::Start)
            }.padding(12.0).background(Rectangle::new(CARD).corner_radius(6.0))
                .reuse(CacheHint::Stable)
        }
    }
}
