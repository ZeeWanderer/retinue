use crate::environment::EnvironmentKey;
use retinue_render::{
    Color,
    painter::{CachedFont, GlyphCache, load_overlay_font},
};
use std::rc::Rc;

mod paragraph;
pub use paragraph::{Paragraph, TextAlignment, TextLayout};

#[derive(Clone)]
pub struct TextStyle {
    pub font: Rc<CachedFont>,
    pub size: f32,
    pub color: Color,
}

impl TextStyle {
    pub fn new(font: Rc<CachedFont>, size: f32, color: Color) -> Self {
        Self { font, size, color }
    }
}

impl PartialEq for TextStyle {
    fn eq(&self, other: &Self) -> bool {
        self.font.id() == other.font.id() && self.size == other.size && self.color == other.color
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        thread_local! {
            static DEFAULT_FONT: Rc<CachedFont> = Rc::new(
                load_overlay_font(&GlyphCache::new(2 * 1024 * 1024)).expect("bundled UI font"));
        }
        Self::new(DEFAULT_FONT.with(Clone::clone), 16.0, Color::rgb(0xffffff))
    }
}

pub struct Typography;
impl EnvironmentKey for Typography {
    type Value = TextStyle;
    fn default_value() -> TextStyle {
        TextStyle::default()
    }
}
