use super::panel::Node;
use retinue_render::Key;
use retinue_ui::authoring::*;

pub(super) fn slot(node: Node, width: f32, height: f32) -> impl Body {
    empty()
        .frame(Frame::new().width(width).height(height))
        .named(Key::from(node))
}
