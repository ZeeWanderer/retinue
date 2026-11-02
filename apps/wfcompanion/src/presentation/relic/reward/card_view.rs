use super::{
    card_layout,
    reward_geometry::{CardSpec, Slot},
};
use retinue_render::Key;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(super) struct CardView {
    pub index: usize,
    pub spec: CardSpec,
}

impl View for CardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let i = self.index;
        ui! {
            LayoutView(card_layout::CardLayout) {
                empty().fill_width().fill_height().id(Key::from(Slot::Name(i)))
                HStack(0.0) {
                    empty().frame(self.spec.platinum_width, 28.0).id(Key::from(Slot::Platinum(i)))
                        .padding(Insets { left: 4.0, ..Insets::default() })
                    if self.spec.vaulted { empty().frame(36.0, 30.0).id(Key::from(Slot::Vaulted(i))) }
                    if self.spec.favorite { empty().frame(27.0, 27.0).id(Key::from(Slot::Favorite(i))) }
                    empty().frame(self.spec.ducats_width, 30.0).id(Key::from(Slot::Ducats(i)))
                        .padding(Insets { right: 4.0, ..Insets::default() })
                }.distribution(Distribution::SpaceAround).id(Key::from(Slot::Prices(i)))
                empty().fill_width().fill_height().id(Key::from(Slot::Ownership(i)))
                empty().fill_width().fill_height().id(Key::from(Slot::Components(i)))
            }.padding(6.0)
        }
    }
}
