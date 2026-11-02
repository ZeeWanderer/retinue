//! Value descriptions of active UI, retained by `runtime::ViewTree`.

mod animated_canvas;
mod children;
mod context;
mod decoration;
mod element;
mod environment;
mod frame;
mod image;
mod modifiers;
pub mod primitive;
mod properties;
mod rectangle;
mod scroll_viewport;
mod stack;
pub(crate) mod stack_layout;
mod state;
mod switch;
mod text;

pub use animated_canvas::AnimatedCanvas;
pub use children::IntoChildren;
pub use context::ViewContext;
pub use element::{Element, IntoElement};
pub use image::Image;
pub use modifiers::{Insets, ViewExt};
pub use primitive::{Canvas, Primitive, PrimitiveOutput};
pub use rectangle::Rectangle;
pub use scroll_viewport::{ScrollAxes, ScrollViewport};
pub use stack::{Alignment, Distribution, HStack, LayoutView, VStack, ZStack};
pub use state::ViewProperties;
pub use switch::Switch;
pub use text::{FirstTextBaseline, LastTextBaseline, Text};

pub(crate) use context::Owned;
pub(crate) use element::{Kind, LayoutDescription, LayoutKind};

/// A composite description with comparable inputs and tracked body dependencies.
///
/// `PartialEq` must cover inputs that affect the body. Equal descriptions skip
/// reevaluation; an observed state change can still invalidate the body. Do not
/// start tasks or mutate application state while describing content. Keep those
/// operations in event handlers or an owning service.
///
/// ```
/// use retinue_ui::prelude::*;
///
/// #[derive(PartialEq)]
/// struct Counter { count: State<u32> }
///
/// impl View for Counter {
///     fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
///         Text::new(format!("Count: {}", self.count.get())).padding(8.0)
///     }
/// }
/// ```
///
/// Custom layout implements [`crate::layout::Layout`]. Direct drawing implements
/// [`Primitive`] or uses [`Canvas`]; neither requires an artificial composite body.
pub trait View: PartialEq + 'static {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement;
}

pub fn empty() -> Element {
    Element::new(Kind::Empty)
}
