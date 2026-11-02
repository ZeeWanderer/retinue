use retinue_ui::prelude::*;

#[observable]
pub struct Model {
    pub title: String,
    pub artwork: ImageResource,
}

pub fn text_style() -> TextStyle {
    TextStyle {
        size: 17.0,
        color: Color::rgb(0xe7edf1),
        ..TextStyle::default()
    }
}
