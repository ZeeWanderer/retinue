use super::{
    card_view,
    reward_geometry::{CardSpec, Slot},
};
use crate::presentation::geometry::Rect;
use retinue_render::Key;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(super) struct RewardView {
    pub window: Rect,
    pub cards: Vec<CardSpec>,
}

impl View for RewardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let width = self.window.width.saturating_sub(34) as f32;
        let height = self.window.height.saturating_sub(34) as f32;
        let footer = height * 0.16;
        let holder = height - footer;
        let cards_width = (width - 14.0).max(0.0);
        let cards_height = (holder - 14.0).max(0.0);
        let gap = cards_width * 0.005;
        let count = self.cards.len();
        let card_width = ((cards_width - count.saturating_sub(1) as f32 * gap)
            / count.max(1) as f32)
            .min(cards_width * 0.25)
            .max(0.0);
        ui! {
            VStack(0.0) {
                HStack(gap) {
                    for (index, spec) in self.cards.iter().enumerate() {
                        card_view::CardView { index, spec: *spec }
                            .frame(card_width, cards_height).id(Key::from(Slot::Card(index)))
                    }
                }.frame(cards_width, cards_height).id(Key::from(Slot::Cards))
                    .padding(7.0).id(Key::from(Slot::Holder))
                empty().frame(width, footer).id(Key::from(Slot::Footer))
            }.padding(2.0).id(Key::from(Slot::Shell))
                .padding(15.0).offset(self.window.x as f32, self.window.y as f32)
                .id(Key::from(Slot::Root))
        }
    }
}
