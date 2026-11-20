use super::{
    Body, Compiler, NodeSchema, Resolution, Select, ViewContext,
    compiler::{First, Second},
    layout_root::LayoutRoot,
};
use crate::{
    ir::{Kind, Property, Result, Value},
    layout::{LayoutAlgorithm, anchors::AnchorPoint},
    runtime::ViewId,
    scene::Algorithm,
};

pub struct Overlay<C, O> {
    pub(super) content: C,
    pub(super) overlay: O,
    pub(super) alignment: AnchorPoint,
}

impl<C: Body, O: Body> Body for Overlay<C, O> {
    type Values = (AnchorPoint, (C::Values, O::Values));
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: &[LayoutRoot::<C>::SCHEMA, LayoutRoot::<O>::SCHEMA],
    };
    const NODES: usize = 1 + LayoutRoot::<C>::NODES + LayoutRoot::<O>::NODES;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let root = compiler.node();
        compiler.bind::<First<S>>(root, Property::Layout, |alignment, _, _| {
            Value::Layout(
                Default::default(),
                Algorithm::Custom(LayoutAlgorithm::new(crate::view::decoration::Decoration {
                    primary: 0,
                    alignment: *alignment,
                })),
            )
        });
        LayoutRoot::<C>::bind::<P, First<Second<S>>>(compiler);
        LayoutRoot::<O>::bind::<P, Second<Second<S>>>(compiler);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        resolution.at_layout(|resolution| {
            Ok((
                self.alignment,
                (
                    LayoutRoot(self.content).resolve(context, ViewId(id.0 + 1), resolution)?,
                    LayoutRoot(self.overlay).resolve(
                        context,
                        ViewId(id.0 + 1 + LayoutRoot::<C>::NODES as u64),
                        resolution,
                    )?,
                ),
            ))
        })
    }
}
