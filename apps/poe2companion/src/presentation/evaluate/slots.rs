use super::panel::Node;
use retinue_render::Key;
use retinue_ui::prelude::*;

pub(super) fn slot(node: Node, width: f32, height: f32) -> Element {
    empty().frame(width, height).id(Key::from(node))
}
