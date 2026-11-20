use super::{Alignment, Distribution, Rectangle, ViewContext, decoration, primitive, stack_layout};
mod children;
mod element;
mod environment;
mod modifiers;
mod properties;
mod scroll_viewport;
mod stack;
mod switch;

pub use children::IntoChildren;
pub use element::{Element, IntoElement};
pub(crate) use element::{ErasedView, Kind, LayoutDescription, LayoutKind};
pub(crate) use environment::Provider;
pub use modifiers::{Insets, ViewExt};
pub(crate) use properties::{Attributes, LayoutInput};
pub use scroll_viewport::{ScrollAxes, ScrollViewport};
pub use stack::{HStack, LayoutView, VStack, ZStack};
pub use switch::Switch;

/// A composite description with comparable inputs and tracked body dependencies.
///
/// `PartialEq` must cover inputs that affect the body. Equal descriptions skip
/// reevaluation; an observed state change can still invalidate the body. Do not
/// start tasks or mutate application state while describing content. Keep those
/// operations in event handlers or an owning service.
///
/// ```
/// use retinue_ui::runtime::reference::prelude::*;
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
/// Custom layout implements [`crate::layout::Layout`]. Custom drawing uses
/// [`super::Canvas`] with [`crate::drawing::CanvasContent`]. [`super::Primitive`] is the lower-level
/// resolution contract for SDK primitives, not a drawing callback on a composite view.
pub trait View: PartialEq + 'static {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement;
}

pub fn empty() -> Element {
    Element::new(Kind::Empty)
}
