use super::{
    Body, Compiler, NodeSchema, Resolution, Select, ViewContext,
    compiler::{First, Second, hit_roots},
    layout_root::LayoutRoot,
};
use crate::{
    Key,
    ir::{Property, Result, Value},
    runtime::ViewId,
};

/// A host-visible geometry name. This does not change retained view identity.
pub struct Named<C> {
    pub(super) content: C,
    pub(super) name: Key,
}

impl<C: Body> Body for Named<C> {
    type Values = (Key, C::Values);
    const SCHEMA: NodeSchema = LayoutRoot::<C>::SCHEMA;
    const NODES: usize = LayoutRoot::<C>::NODES;

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let start = compiler.current();
        LayoutRoot::<C>::bind::<P, Second<S>>(compiler);
        let roots = hit_roots(start, Self::SCHEMA);
        assert_eq!(roots.len(), 1);
        compiler.override_binding::<First<S>>(roots[0], Property::Name, |name, _, _| {
            Value::Name(Some(*name))
        });
    }

    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((
            self.name,
            LayoutRoot(self.content).resolve(context, id, resolution)?,
        ))
    }
}
