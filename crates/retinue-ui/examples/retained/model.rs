use retinue_ui::prelude::*;

#[observable]
pub struct Model {
    pub filled: u32,
    pub details: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self::new(2, true)
    }
}

pub struct Accent;

impl EnvironmentKey for Accent {
    type Value = Color;
    fn default_value() -> Color {
        Color::rgb(0x52be8c)
    }
}
