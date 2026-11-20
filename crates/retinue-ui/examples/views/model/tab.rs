use super::{ACCENT, CARD};
use retinue_ui::{
    authoring::*,
    prelude::{Animation, Easing, State},
};
use std::time::Duration;

#[derive(PartialEq)]
pub(super) struct Tab {
    pub selected: State<u64>,
    pub number: u64,
    pub title: &'static str,
}

#[view]
impl View for Tab {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
        let active = self.selected.get() == self.number;
        let animation = Animation::new(Duration::from_millis(170)).easing(Easing::InOutCubic);
        let width = context.animated_with("width", if active { 127.0 } else { 36.0 }, animation);
        let color = context.animated_with("color", if active { ACCENT } else { CARD }, animation);
        Text::new(self.title)
            .line_limit(1)
            .frame(Frame::new().width(width).height(36.0))
            .background(Rectangle::new(color).corner_radius(18.0))
            .clipped()
            .interactive()
    }
}
