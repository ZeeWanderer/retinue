use super::label_view::LabelView;
use super::row_layout::RowLayout;
use crate::presentation::widgets::compiled::{frame, slot};
use retinue_render::Key;
use retinue_ui::authoring::*;

#[derive(Clone, PartialEq)]
pub(in crate::presentation::settings) struct RowView {
    pub id: Key,
    pub labels: Vec<LabelView>,
    pub button: Option<Key>,
    pub scale: f32,
}

#[view]
impl View for RowView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            LayoutView(RowLayout { scale: self.scale }) {
                VStack(5.0 * self.scale) {
                    ForEach(self.labels.iter(), |label| label.id) { |label| compose(label.clone()) }
                }.stretch_children()
                if let Some(button) = self.button {
                    slot(button).frame(frame(80.0 * self.scale, 34.0 * self.scale))
                }
            }.named(self.id)
        }
    }
}
