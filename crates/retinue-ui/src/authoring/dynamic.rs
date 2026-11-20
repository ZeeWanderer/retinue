use super::{Body, Children, Compiler, NodeSchema, Resolution, Select, ViewContext};
use crate::{
    Key,
    ir::{Kind, Result},
    runtime::ViewId,
};

pub struct Group<C>(pub C);

impl<C: Children> Body for Group<C> {
    type Values = C::Values;
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Group,
        children: C::SCHEMAS,
    };
    const NODES: usize = 1 + C::NODES;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        compiler.node();
        C::bind::<P, S>(compiler);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        self.0.resolve(context, ViewId(id.0 + 1), resolution)
    }
}

pub enum Conditional<A, B> {
    Then(A),
    Else(B),
}

impl<A: Body, B: Body> Body for Conditional<A, B> {
    type Values = ();
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Slot,
        children: &[],
    };
    const NODES: usize = 1;
    fn bind<P: 'static, S: Select<P, Value = ()>>(compiler: &mut Compiler<P>) {
        compiler.node();
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<()> {
        match self {
            Self::Then(body) => {
                resolution.children(context, id, vec![(Key::new("ui.branch", 0), body)])
            }
            Self::Else(body) => {
                resolution.children(context, id, vec![(Key::new("ui.branch", 1), body)])
            }
        }
    }
}

impl<B: Body> Body for Option<B> {
    type Values = ();
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Slot,
        children: &[],
    };
    const NODES: usize = 1;
    fn bind<P: 'static, S: Select<P, Value = ()>>(compiler: &mut Compiler<P>) {
        compiler.node();
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<()> {
        match self {
            Some(body) => resolution.children(context, id, vec![(Key::new("ui.branch", 0), body)]),
            None => resolution.clear(id),
        }
    }
}

pub struct ForEach<B> {
    children: Vec<(Key, B)>,
}

/// Keys are local to this collection and preserve row state across reorders.
#[allow(non_snake_case)]
pub fn ForEach<I: IntoIterator, K: Into<Key>, B: Body>(
    items: I,
    mut key: impl FnMut(&I::Item) -> K,
    mut content: impl FnMut(I::Item) -> B,
) -> ForEach<B> {
    ForEach {
        children: items
            .into_iter()
            .map(|item| (key(&item).into(), content(item)))
            .collect(),
    }
}

impl<B: Body> Body for ForEach<B> {
    type Values = ();
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Slot,
        children: &[],
    };
    const NODES: usize = 1;
    fn bind<P: 'static, S: Select<P, Value = ()>>(compiler: &mut Compiler<P>) {
        compiler.node();
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<()> {
        resolution.children(context, id, self.children)
    }
}
