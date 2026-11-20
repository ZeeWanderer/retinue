use retinue_render::Key;
pub(crate) mod compiled;
pub(crate) mod menu;
pub(crate) mod search;

mod asset_image;
mod component_image;
mod leased_image;
pub(crate) use asset_image::AssetImage;
mod fonts;
mod text;
pub(crate) use component_image::ComponentImage;
mod loading_placeholder;
pub use fonts::Fonts;
pub(crate) use loading_placeholder::LoadingPlaceholder;
use std::{hash::Hash, hash::Hasher};
pub(super) use text::fit_text;

pub fn key(scope: &'static str, id: impl Hash) -> Key {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    Key::new(scope, hash.finish())
}
