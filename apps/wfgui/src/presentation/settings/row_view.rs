use super::row_layout::RowLayout;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(in crate::presentation::settings) struct RowView {
    pub labels: Vec<Element>,
    pub button: Element,
    pub scale: f32,
}

impl View for RowView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            LayoutView(RowLayout { scale: self.scale }) {
                VStack(5.0 * self.scale) {
                    for label in &self.labels { label.clone() }
                }.alignment(Alignment::Stretch)
                self.button.clone()
            }
        }
    }
}
