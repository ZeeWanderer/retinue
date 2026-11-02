use crate::presentation::widgets::slot;
use retinue_render::Key;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(in crate::presentation::settings) struct SectionView {
    pub id: Key,
    pub children: Vec<Element>,
}

impl View for SectionView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            VStack(0.0) {
                for child in &self.children { child.clone() }
            }.alignment(Alignment::Stretch).fill_width().background { slot(self.id) }
        }
    }
}
