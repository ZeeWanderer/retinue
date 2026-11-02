use retinue_render::{Bounds, Key};
use retinue_ui::{layout::anchors::AnchorPoint, prelude::*};
pub(crate) mod menu;
pub(crate) mod search;

mod asset_image;
mod component_image;
mod leased_image;
pub(crate) use asset_image::AssetImage;
mod fitted_label;
mod fonts;
pub(crate) use component_image::ComponentImage;
mod label;
mod loading_placeholder;
pub use fitted_label::aligned_label;
pub(super) use fitted_label::fit_text;
pub use fitted_label::fitted_label;
pub use fonts::Fonts;
pub use label::label;
pub(crate) use loading_placeholder::LoadingPlaceholder;
use std::{hash::Hash, hash::Hasher};

pub fn key(scope: &'static str, id: impl Hash) -> Key {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    Key::new(scope, hash.finish())
}

pub fn slot(id: Key) -> Element {
    crate::presentation::controls::view_host::PaintSlot(id).into_element()
}

pub fn place(view: impl IntoElement, bounds: Bounds) -> Element {
    view.frame_aligned(bounds.width, bounds.height, AnchorPoint::TOP_LEADING)
        .offset(bounds.x, bounds.y)
}

pub fn fill(color: [u8; 4], radius: f32) -> Element {
    Rectangle::new(Color::from_bgra(color))
        .corner_radius(radius)
        .into_element()
}
