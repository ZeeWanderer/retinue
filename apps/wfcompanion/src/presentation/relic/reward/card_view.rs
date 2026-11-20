use super::{
    card_layout,
    reward_geometry::{CardSpec, Slot},
};
use retinue_render::Key;
use retinue_ui::{authoring::*, layout::Insets};

#[derive(PartialEq)]
pub(super) struct CardView {
    pub index: usize,
    pub spec: CardSpec,
}

#[view]
impl View for CardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let i = self.index;
        ui! {
            LayoutView(card_layout::CardLayout) {
                empty().frame(Frame::new().fill_width().fill_height()).named(Key::from(Slot::Name(i)))
                HStack(0.0) {
                    empty().frame(Frame::new().width(self.spec.platinum_width).height(28.0)).named(Key::from(Slot::Platinum(i)))
                        .padding(Insets { left: 4.0, ..Insets::default() })
                    if self.spec.vaulted { empty().frame(Frame::new().width(36.0).height(30.0)).named(Key::from(Slot::Vaulted(i))) }
                    if self.spec.favorite { empty().frame(Frame::new().width(27.0).height(27.0)).named(Key::from(Slot::Favorite(i))) }
                    empty().frame(Frame::new().width(self.spec.ducats_width).height(30.0)).named(Key::from(Slot::Ducats(i)))
                        .padding(Insets { right: 4.0, ..Insets::default() })
                }.distribution(Distribution::SpaceAround).named(Key::from(Slot::Prices(i)))
                empty().frame(Frame::new().fill_width().fill_height()).named(Key::from(Slot::Ownership(i)))
                empty().frame(Frame::new().fill_width().fill_height()).named(Key::from(Slot::Components(i)))
            }.padding(6.0)
        }
    }
}
