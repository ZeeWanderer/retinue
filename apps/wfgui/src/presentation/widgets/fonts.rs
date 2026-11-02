use crate::{presentation::controls::ControlHost, presentation::controls::Weight};
use retinue_render::painter::CachedFont;
use std::rc::Rc;

#[derive(Clone)]
pub struct Fonts {
    pub regular: Rc<CachedFont>,
    pub light: Rc<CachedFont>,
    pub medium: Rc<CachedFont>,
    pub bold: Rc<CachedFont>,
}

impl Fonts {
    pub fn new(ui: &ControlHost) -> Self {
        Self {
            regular: ui.font_resource(Weight::Regular),
            light: ui.font_resource(Weight::Light),
            medium: ui.font_resource(Weight::Medium),
            bold: ui.font_resource(Weight::Bold),
        }
    }
}

impl PartialEq for Fonts {
    fn eq(&self, other: &Self) -> bool {
        self.regular.id() == other.regular.id()
            && self.light.id() == other.light.id()
            && self.medium.id() == other.medium.id()
            && self.bold.id() == other.bold.id()
    }
}
