use super::{
    panel::{CONTENT, Node},
    slots::slot,
};
use retinue_render::Key;
use retinue_ui::{
    authoring::*,
    layout::{Insets, anchors::AnchorPoint},
};

pub(super) fn item_view(scale: f32, rows: &[(usize, f32)], height: f32) -> impl Body + use<> {
    ui! {
        VStack(0.0) {
            ForEach(rows.iter().copied(), |(index, _)| Key::from(Node::Row(*index))) { |(index, height)|
                slot(Node::Row(index), CONTENT * scale, height)
            }
        }.alignment(HorizontalAlignment::Leading)
            .padding(Insets { top: 6.0 * scale, ..Insets::default() })
            .frame(Frame::new().width(CONTENT * scale).height(height).alignment(AnchorPoint::TOP_LEADING))
    }
}
