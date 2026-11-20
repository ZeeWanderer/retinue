use super::{
    Body, Children, Compiler, NodeSchema, Resolution, Select, ViewContext,
    compiler::{First, Second},
};
use crate::{
    ir::{Kind, Property, Result, Value},
    layout::{LayoutAlgorithm, LayoutSize, positioned::Positioned},
    runtime::ViewId,
    scene::Algorithm,
};

/// A fixed local coordinate space. Unplaced children fill it; placed children use their own bounds.
pub struct PositionedStack<C> {
    size: LayoutSize,
    children: C,
}

#[allow(non_snake_case)]
pub fn PositionedStack<C: Children>(size: LayoutSize, children: C) -> PositionedStack<C> {
    PositionedStack { size, children }
}

impl<C: Children> PositionedStack<C> {
    /// Place named children from the parent. Unlisted children keep their own placement.
    pub fn child_frames(
        self,
        frames: impl IntoIterator<Item = (crate::Key, crate::Bounds)>,
    ) -> super::LayoutView<C> {
        super::LayoutView::new(
            crate::layout::positioned::ChildFrames {
                size: self.size,
                frames: frames.into_iter().collect(),
            },
            self.children,
        )
    }
}

impl<C: Children> Body for PositionedStack<C> {
    type Values = (LayoutSize, C::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: C::SCHEMAS,
    };
    const NODES: usize = 1 + C::NODES;
    const BACKGROUND_TARGET: Option<crate::ir::Local> = Some(crate::ir::Local(0));

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<First<S>>(node, Property::Layout, |&size, _, _| {
            Value::Layout(
                Default::default(),
                Algorithm::Custom(LayoutAlgorithm::new(Positioned::new(size))),
            )
        });
        C::bind::<P, Second<S>>(compiler);
    }

    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((
            self.size,
            resolution.at_layout(|r| self.children.resolve(context, ViewId(id.0 + 1), r))?,
        ))
    }
}
