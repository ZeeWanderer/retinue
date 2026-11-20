use super::{Body, Compiler, NodeSchema, Resolution, Select, ViewContext};
use crate::{
    ir::{Kind, Recipe, Result},
    layout::{LayoutAlgorithm, anchors::AnchorPoint, layered::Layered},
    runtime::ViewId,
    scene::Algorithm,
};

const fn roots(schema: NodeSchema) -> usize {
    match schema.kind {
        Kind::Slot => 2, // The selected branch can have zero or multiple roots.
        Kind::Group => {
            let mut count = 0;
            let mut index = 0;
            while index < schema.children.len() {
                count += roots(schema.children[index]);
                if count > 1 {
                    return 2;
                }
                index += 1;
            }
            count
        }
        _ => 1,
    }
}

pub(super) struct LayoutRoot<C>(pub C);

impl<C: Body> Body for LayoutRoot<C> {
    type Values = C::Values;
    const SCHEMA: NodeSchema = if roots(C::SCHEMA) == 1 {
        C::SCHEMA
    } else {
        NodeSchema {
            kind: Kind::Layout,
            children: &[C::SCHEMA],
        }
    };
    const NODES: usize = C::NODES + (roots(C::SCHEMA) != 1) as usize;

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        if Self::NODES != C::NODES {
            let node = compiler.node();
            compiler.recipe(
                node,
                Recipe {
                    algorithm: Algorithm::Custom(LayoutAlgorithm::new(Layered {
                        alignment: AnchorPoint::CENTER,
                    })),
                    ..Default::default()
                },
            );
        }
        C::bind::<P, S>(compiler);
    }

    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        if Self::NODES == C::NODES {
            self.0.resolve(context, id, resolution)
        } else {
            resolution.at_layout(|resolution| self.0.resolve(context, ViewId(id.0 + 1), resolution))
        }
    }
}
