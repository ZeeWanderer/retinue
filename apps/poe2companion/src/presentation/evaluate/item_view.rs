use super::{
    panel::{CONTENT, Node},
    slots::slot,
};
use retinue_ui::{layout::anchors::AnchorPoint, prelude::*};

#[derive(PartialEq)]
pub(super) struct ItemView {
    pub scale: f32,
    pub rows: Vec<(usize, f32)>,
    pub height: f32,
}

impl View for ItemView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            VStack(0.0) {
                for (index, height) in &self.rows {
                    slot(Node::Row(*index), CONTENT * self.scale, *height)
                }
            }.alignment(Alignment::Start)
                .padding(Insets { top: 6.0 * self.scale, ..Insets::default() })
                .frame_aligned(CONTENT * self.scale, self.height, AnchorPoint::TOP_LEADING)
        }
    }
}
