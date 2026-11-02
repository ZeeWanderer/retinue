use super::{ACCENT, CARD};
use retinue_ui::prelude::*;
use std::time::Duration;

#[derive(PartialEq)]
pub(super) struct Tab {
    pub selected: State<u64>,
    pub number: u64,
    pub title: &'static str,
}

impl View for Tab {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let active = self.selected.get() == self.number;
        Text::new(self.title)
            .line_limit(1)
            .frame(if active { 127.0 } else { 36.0 }, 36.0)
            .background(Rectangle::new(if active { ACCENT } else { CARD }).corner_radius(18.0))
            .clipped()
            .interactive()
            .animation(
                Animation::new(Duration::from_millis(170)).easing(Easing::InOutCubic),
                active,
            )
    }
}
