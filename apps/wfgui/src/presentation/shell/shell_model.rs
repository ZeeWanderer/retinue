use super::{Page, composition};
use retinue_ui::{Key, drawing::DrawList, prelude::*};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub struct Viewport {
    pub size: (u32, u32),
    pub scale: f32,
}

impl Viewport {
    pub fn width(self) -> f32 {
        self.size.0 as f32 / self.scale
    }
    pub fn height(self) -> f32 {
        self.size.1 as f32 / self.scale
    }
    pub fn body(self) -> f32 {
        (self.height() - 46.0).max(0.0)
    }
}

impl EnvironmentKey for Viewport {
    type Value = Self;
    fn default_value() -> Self {
        Self {
            size: (0, 0),
            scale: 1.0,
        }
    }
}

#[observable]
pub struct ShellModel {
    pub viewport: Viewport,
    pub left: f32,
    pub right: f32,
    pub right_fits: bool,
    pub compact_left: bool,
    pub right_available: bool,
    pub tooltip: bool,
    pub page: Page,
    #[observation(ignore)]
    pub paints: HashMap<Key, State<DrawList>>,
}

impl Default for ShellModel {
    fn default() -> Self {
        use composition::*;
        Self::new(
            Viewport::default_value(),
            0.0,
            0.0,
            true,
            false,
            true,
            false,
            Page::Foundry,
            [TITLE, LEFT, CENTER, RIGHT, TOOLTIP]
                .into_iter()
                .chain(CONTROLS.map(control_key))
                .map(|key| (key, State::new(DrawList::default())))
                .collect(),
        )
    }
}

impl ShellModel {
    pub fn paint(&self, key: Key) -> State<DrawList> {
        self.paints.read()[&key].clone()
    }
}
