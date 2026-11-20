use super::{
    Body, Compiler, NodeSchema, Select, ViewContext,
    compiler::{First, Second},
};
use crate::{
    Key,
    ir::{Handle, InstanceGraph, Local, Result},
    runtime::ViewId,
    view::Owned,
};

pub struct Resolution<'a> {
    pub(crate) graph: &'a mut InstanceGraph,
    pub(crate) owner: Handle,
    pub(crate) base: ViewId,
    pub(crate) hit_testing: Option<bool>,
}

impl Resolution<'_> {
    pub(super) fn at_layout<T>(&mut self, resolve: impl FnOnce(&mut Self) -> T) -> T {
        let policy = self.hit_testing.take();
        let result = resolve(self);
        self.hit_testing = policy;
        result
    }

    pub(super) fn with_hit_testing<T>(
        &mut self,
        enabled: bool,
        resolve: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let policy = self.hit_testing;
        self.hit_testing = policy.or(Some(enabled));
        let result = resolve(self);
        self.hit_testing = policy;
        result
    }

    pub(super) fn clear(&mut self, id: ViewId) -> Result<()> {
        self.graph
            .children(self.owner, Local((id.0 - self.base.0) as u32), Vec::new())
    }

    pub(super) fn children<B: Body>(
        &mut self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        children: Vec<(Key, B)>,
    ) -> Result<()> {
        let program = self.graph.body_template::<Scoped<B>>()?;
        let keys: Vec<_> = children.iter().map(|(key, _)| *key).collect();
        let handles = self.graph.instances(
            self.owner,
            Local((id.0 - self.base.0) as u32),
            &keys,
            program,
            || None,
        )?;
        let hit_testing = self.hit_testing;
        for ((_, body), handle) in children.into_iter().zip(handles) {
            let base = ViewId(self.graph.key(handle, Local(0))?.value);
            self.graph.with_state::<Owned, _>(handle, |graph, owned| {
                let mut child_context = ViewContext {
                    id: base,
                    owned,
                    environment: context.environment,
                    mutation: context.mutation,
                    time: context.time,
                    deadlines: context.deadlines,
                    frame_interval: context.frame_interval,
                    motion_enabled: context.motion_enabled,
                };
                let value = Scoped { hit_testing, body }.resolve(
                    &mut child_context,
                    base,
                    &mut Resolution {
                        graph,
                        owner: handle,
                        base,
                        hit_testing,
                    },
                )?;
                graph.stage(handle, Some(value))
            })?;
        }
        Ok(())
    }
}

pub(super) struct Scoped<B> {
    pub(super) hit_testing: Option<bool>,
    pub(super) body: B,
}

impl<B: Body> Body for Scoped<B> {
    type Values = (Option<bool>, B::Values);
    const SCHEMA: NodeSchema = B::SCHEMA;
    const NODES: usize = B::NODES;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let root = compiler.current();
        B::bind::<P, Second<S>>(compiler);
        compiler.inherited_hit_testing::<First<S>>(root, B::SCHEMA);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((
            self.hit_testing,
            self.body.resolve(context, id, resolution)?,
        ))
    }
}
