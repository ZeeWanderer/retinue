use super::{Action, Filter, Foundry, Rect};
use retinue_ui::Key;

pub const PANEL: Key = Key::new("foundry.filters", 0);
pub const CONTENT: Key = Key::new("foundry.filters", 1);

#[derive(Clone, Copy, PartialEq)]
pub struct Geometry {
    pub bounds: Rect,
    pub viewport: Rect,
    pub scale: f32,
}

impl Geometry {
    pub fn new(view: &Foundry) -> Option<Self> {
        view.filters_open().then(|| Self {
            bounds: view.filter_panel.popup.bounds,
            viewport: view.filter_viewport(),
            scale: view.scale,
        })
    }

    pub fn px(self, value: f32) -> u32 {
        (value * self.scale).round() as u32
    }

    pub fn width(self) -> f32 {
        self.viewport.width as f32 / self.scale
    }

    pub fn choice_width(self) -> f32 {
        (self.width() - 10.0).max(0.0) / 2.0
    }
}

pub fn choice_key(filter: Filter, value: bool) -> Key {
    Key::new(
        "foundry.filter.choice",
        filter as u64 * 2 + u64::from(!value),
    )
}

pub fn action(key: Key) -> Option<Action> {
    match key.namespace {
        "foundry.filter.choice" => Filter::ALL
            .get(key.value as usize / 2)
            .map(|filter| Action::FilterChoice(*filter, key.value % 2 == 0)),
        "foundry.filter.close" => Some(Action::CloseFilters),
        "foundry.filter.clear" => Some(Action::ClearFilters),
        "foundry.filter.scroll" => Some(Action::FilterScroll),
        _ => None,
    }
}
