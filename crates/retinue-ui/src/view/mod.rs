//! Shared view primitives and observation context for compiled authoring.

mod alignment;
mod animated_canvas;
mod context;
pub(crate) mod decoration;
mod image;
pub mod primitive;
mod rectangle;
#[cfg(any(test, feature = "reference-frontend"))]
mod reference;
pub(crate) mod stack_layout;
mod state;
mod text;

pub use alignment::{Alignment, Distribution};
pub use animated_canvas::AnimatedCanvas;
pub use context::ViewContext;
pub use image::Image;
pub use primitive::{Canvas, Primitive, PrimitiveOutput};
pub use rectangle::Rectangle;
pub use state::ViewProperties;
pub use text::{FirstTextBaseline, LastTextBaseline, Text};

pub(crate) use context::Owned;
#[cfg(any(test, feature = "reference-frontend"))]
pub use reference::*;
