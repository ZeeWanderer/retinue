use super::{
    card_view,
    reward_geometry::{CardSpec, Slot},
};
use crate::presentation::geometry::Rect;
use retinue_render::Key;
use retinue_ui::authoring::*;

#[derive(PartialEq)]
pub(super) struct RewardView {
    pub window: Rect,
    pub cards: Vec<CardSpec>,
}

#[view]
impl View for RewardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
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
                    ForEach(self.cards.iter().copied().enumerate(), |(i, _)| Key::from(Slot::Card(*i))) { |(index, spec)|
                        compose(card_view::CardView { index, spec })
                            .frame(Frame::new().width(card_width).height(cards_height)).named(Key::from(Slot::Card(index)))
                    }
                }.frame(Frame::new().width(cards_width).height(cards_height)).named(Key::from(Slot::Cards))
                    .padding(7.0).named(Key::from(Slot::Holder))
                empty().frame(Frame::new().width(width).height(footer)).named(Key::from(Slot::Footer))
            }.padding(2.0).named(Key::from(Slot::Shell))
                .padding(15.0).offset(self.window.x as f32, self.window.y as f32)
                .named(Key::from(Slot::Root))
        }
    }
}
