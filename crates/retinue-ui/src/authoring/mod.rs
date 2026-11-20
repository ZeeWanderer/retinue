//! Typed authoring compiled directly to shared view IR templates.
//!
//! Structural types and their [`NodeSchema`] constants describe the layout at compile
//! time. The first mount materializes the flat template; later instances share it.
//! Each body evaluation produces one typed value record, not recursive Elements.
//! Dynamic branches and keyed collections splice instances into those templates.

mod children;
pub mod compiler;
mod composite;
mod dynamic;
mod item_views;
mod layout_root;
mod metadata;
mod modifiers;
mod named;
mod overlay;
mod positioned;
mod primitives;
mod resolve;
mod scopes;
mod scroll;
mod stacks;

#[cfg(test)]
mod tests;

pub use crate::view::{Canvas, Distribution, Image, Rectangle, Text, ViewContext};
pub use children::Children;
pub use compiler::{Compiler, Select};
pub use composite::compose;
pub use dynamic::{Conditional, ForEach, Group};
pub use item_views::{ItemView, ItemViews};
pub use modifiers::{Frame, Framed, ViewExt};
pub use named::Named;
pub use positioned::PositionedStack;
pub use resolve::Resolution;
pub use retinue_ui_macros::{compiled_ui as ui, compiled_view as view};
pub use scopes::InputScope;
pub use scroll::{ScrollAxes, ScrollViewport};
pub use stacks::{HStack, HorizontalAlignment, LayoutView, VStack, VerticalAlignment, ZStack};

use crate::{
    environment::Environment,
    ir::{Handle, InstanceGraph, Kind, Result},
};
use std::{any::TypeId, rc::Rc};

#[derive(Clone, Copy)]
pub struct NodeSchema {
    pub kind: Kind,
    pub children: &'static [NodeSchema],
}

/// Structural authoring value. Custom drawing belongs in Canvas, not this trait.
pub trait Body: 'static {
    type Values: PartialEq + 'static;
    const SCHEMA: NodeSchema;
    const NODES: usize;
    /// Unpainted layout root that preserves a parent's background position and eligibility.
    const BACKGROUND_TARGET: Option<crate::ir::Local> = None;
    const BOUNDS_BACKGROUND: bool = false;

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>);
    fn bind_background<P: 'static, S: Select<P, Value = Self::Values>>(
        _: &mut Compiler<P>,
        _: crate::ir::Local,
    ) {
        unreachable!("body does not support bounds-only decoration")
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values>;
}

/// Reusable content described by a [`Body`]. State read while evaluating the
/// body determines which changes cause the mounted view to update.
pub trait View: 'static {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body + use<Self>;
}

/// An empty body, useful when a conditional branch has no content.
pub fn empty() -> impl Body {
    ()
}

#[allow(non_snake_case)]
/// Displays a string. Use modifiers such as `.font_size(...)` to style it;
/// put it in a stack to align it with other content.
pub fn Text(text: impl Into<std::sync::Arc<str>>) -> Text {
    Text::new(text)
}

#[allow(non_snake_case)]
/// Records custom drawing inside a layout slot. Use this for content that is
/// easier to draw than to express as text, images or shapes.
pub fn Canvas<C: crate::drawing::CanvasContent>(content: C) -> Canvas<C> {
    Canvas::new(content)
}

#[allow(non_snake_case)]
/// Displays an image resource. Layout can reserve its slot before pixels arrive.
pub fn Image(resource: crate::image::ImageResource) -> Image {
    Image::new(resource)
}

#[allow(non_snake_case)]
/// Creates a solid rectangle. Use it as a background or give it an explicit frame.
pub fn Rectangle(color: retinue_render::Color) -> Rectangle {
    Rectangle::new(color)
}

#[allow(non_snake_case)]
/// Layers children back to front. By default, children share the same center.
///
/// Put a background first and the content that must remain visible after it.
///
/// ```
/// use retinue_ui::prelude::*;
///
/// let badge = ui! {
///     ZStack() {
///         Rectangle(Color::rgb(0x253745)).corner_radius(5.0)
///         Text("Ready").padding(4.0)
///     }
/// };
/// ```
pub fn ZStack<C: Children>(children: C) -> ZStack<C> {
    ZStack::new(children)
}

#[allow(non_snake_case)]
/// Clip content to a viewport without prescribing how its pixels are cached.
///
/// The default axis is vertical. Supply the controller's scroll offset with
/// `.position(x, y)`; positive vertical offsets move the content upwards.
pub fn ScrollViewport<C: Body>(content: C) -> ScrollViewport<C> {
    ScrollViewport::new(content)
}

#[allow(non_snake_case)]
/// Arranges children from left to right, with the given gap between neighbors.
///
/// Children are centered vertically by default. Use baseline alignment when
/// labels with different font sizes should sit on the same line.
///
/// ```
/// use retinue_ui::prelude::*;
///
/// let row = ui! {
///     HStack(6.0) {
///         Text("Rank")
///         Text("18").font_size(20.0)
///     }.alignment(VerticalAlignment::FirstTextBaseline)
/// };
/// ```
pub fn HStack<C: Children>(spacing: f32, children: C) -> HStack<C> {
    HStack::new(spacing, children)
}

#[allow(non_snake_case)]
/// Arranges children from top to bottom, with the given gap between neighbors.
///
/// Children are centered horizontally by default. Use `.alignment(...)` to
/// align their edges. Spacing belongs between children; use `.padding(...)`
/// for space around the whole column.
///
/// ```
/// use retinue_ui::prelude::*;
///
/// let summary = ui! {
///     VStack(8.0) {
///         Text("Foundry").font_size(20.0)
///         Text("3 items ready")
///     }.alignment(HorizontalAlignment::Leading)
/// };
/// ```
pub fn VStack<C: Children>(spacing: f32, children: C) -> VStack<C> {
    VStack::new(spacing, children)
}

#[allow(non_snake_case)]
/// Places children using a custom layout algorithm. Prefer the standard stack
/// constructors when ordering, spacing and alignment describe the arrangement.
pub fn LayoutView<L, C>(layout: L, children: C) -> LayoutView<C>
where
    L: crate::layout::Layout + Send + Sync,
    L::Cache: Send,
    C: Children,
{
    LayoutView::new(layout, children)
}

impl InstanceGraph {
    pub fn with_root_view(view: impl View, environment: Environment) -> Result<Self> {
        Self::with_root_body(move |context| view.body(context), environment)
    }

    pub fn with_root_body<B: Body>(
        body: impl FnMut(&mut ViewContext<'_>) -> B + 'static,
        environment: Environment,
    ) -> Result<Self> {
        let mut graph = Self::default();
        let root = graph.mount_body(body, environment)?;
        graph.set_root(Some(root))?;
        Ok(graph)
    }

    pub(crate) fn body_template<B: Body>(
        &mut self,
    ) -> Result<Rc<crate::ir::Template<Option<B::Values>>>> {
        let id = TypeId::of::<B>();
        if let Some(program) = self.templates.get(&id) {
            return Ok(program.clone().downcast().unwrap());
        }
        let program = Rc::new(compiler::compile::<B>()?);
        self.templates.insert(id, program.clone());
        Ok(program)
    }

    pub fn mount_view(&mut self, view: impl View, environment: Environment) -> Result<Handle> {
        self.mount_body(move |context| view.body(context), environment)
    }

    pub fn mount_body<B: Body>(
        &mut self,
        mut body: impl FnMut(&mut ViewContext<'_>) -> B + 'static,
        environment: Environment,
    ) -> Result<Handle> {
        let program = self.body_template::<B>()?;
        let handle = self.mount(program, None);
        self.observe_body(handle, environment, move |context, graph| {
            let id = context.id();
            let body = {
                #[cfg(feature = "profiling")]
                let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Body);
                body(context)
            };
            body.resolve(
                context,
                id,
                &mut Resolution {
                    graph,
                    owner: handle,
                    base: id,
                    hit_testing: None,
                },
            )
            .map(Some)
        })?;
        Ok(handle)
    }
}
