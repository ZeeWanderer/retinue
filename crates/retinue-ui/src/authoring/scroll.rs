use super::{
    Body, Compiler, NodeSchema, Resolution, Select, ViewContext, ViewExt,
    compiler::{First, Second},
    modifiers::Offset,
};
pub use crate::layout::scroll::ScrollAxes;
use crate::{
    ir::{Kind, Local, Property, Result, Value},
    layout::{Insets, LayoutAlgorithm, scroll::ScrollLayout},
    runtime::ViewId,
    scene::Algorithm,
};

/// A clipped viewport; its controller owns scrolling and input policy.
pub struct ScrollViewport<C> {
    content: C,
    axes: ScrollAxes,
    position: (f32, f32),
    insets: Insets,
}

impl<C: Body> ScrollViewport<C> {
    pub fn new(content: C) -> Self {
        Self {
            content,
            axes: ScrollAxes::Vertical,
            position: (0.0, 0.0),
            insets: Insets::default(),
        }
    }
    pub fn axes(mut self, axes: ScrollAxes) -> Self {
        self.axes = axes;
        self
    }
    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.position = (x, y);
        self
    }
    pub fn content_insets(mut self, insets: impl Into<Insets>) -> Self {
        self.insets = insets.into();
        self
    }
}

impl<C: Body> Body for ScrollViewport<C> {
    type Values = ((ScrollAxes, Insets), <Offset<C> as Body>::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: &[Offset::<C>::SCHEMA],
    };
    const NODES: usize = 1 + Offset::<C>::NODES;
    const BACKGROUND_TARGET: Option<Local> = Some(Local(0));

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<First<S>>(node, Property::Layout, |&(axes, insets), _, _| {
            Value::Layout(
                taffy::Style::default(),
                Algorithm::Custom(LayoutAlgorithm::new(ScrollLayout { axes, insets })),
            )
        });
        compiler.bind::<First<S>>(node, Property::Clip, |_, _, _| Value::Clip(true));
        Offset::<C>::bind::<P, Second<S>>(compiler);
    }

    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        let offset = match self.axes {
            ScrollAxes::Vertical => (0.0, -self.position.1),
            ScrollAxes::Horizontal => (-self.position.0, 0.0),
            ScrollAxes::Both => (-self.position.0, -self.position.1),
        };
        Ok((
            (self.axes, self.insets),
            resolution.at_layout(|resolution| {
                self.content.offset(offset.0, offset.1).resolve(
                    context,
                    ViewId(id.0 + 1),
                    resolution,
                )
            })?,
        ))
    }
}
