use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerSurface,
};

pub struct OverlayLayerState {
    anchor: Anchor,
    size: (u32, u32),
    exclusive_zone: i32,
    pub keyboard: KeyboardInteractivity,
}

impl OverlayLayerState {
    pub fn full_output(keyboard: KeyboardInteractivity) -> Self {
        Self {
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            size: (0, 0),
            exclusive_zone: -1,
            keyboard,
        }
    }

    pub fn top_left(size: (u32, u32)) -> Self {
        Self {
            anchor: Anchor::TOP | Anchor::LEFT,
            size,
            exclusive_zone: 0,
            keyboard: KeyboardInteractivity::None,
        }
    }

    pub fn apply(&self, layer: &LayerSurface) {
        // Layer-shell resets all role state on unmap, including the layer itself.
        layer.set_layer(Layer::Overlay);
        layer.set_anchor(self.anchor);
        layer.set_size(self.size.0, self.size.1);
        layer.set_exclusive_zone(self.exclusive_zone);
        layer.set_margin(0, 0, 0, 0);
        layer.set_keyboard_interactivity(self.keyboard);
    }
}
