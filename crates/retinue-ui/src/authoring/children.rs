use super::{Body, Compiler, NodeSchema, Resolution, Select, ViewContext};
use crate::ir::Result;
use std::marker::PhantomData;

pub trait Children: 'static {
    type Values: PartialEq + 'static;
    const SCHEMAS: &'static [NodeSchema];
    const NODES: usize;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>);
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values>;
}

pub struct Field<S, T, const I: usize>(PhantomData<(S, T)>);

macro_rules! field {
    (($($type:ident),+), $index:tt, $target:ident) => {
        impl<P, S, $($type: PartialEq + 'static),+> Select<P> for Field<S, ($($type,)+), $index>
        where S: Select<P, Value = ($($type,)+)> {
            type Value = $target;
            fn get(value: &P) -> Option<&$target> { S::get(value).map(|value| &value.$index) }
        }
    };
}

macro_rules! tuple {
    ($($type:ident : $index:tt),+) => {
        impl<$($type: Body),+> Children for ($($type,)+) {
            type Values = ($($type::Values,)+);
            const SCHEMAS: &'static [NodeSchema] = &[$($type::SCHEMA,)+];
            const NODES: usize = 0 $(+ $type::NODES)+;
            fn bind<P: 'static, S: Select<P, Value = <Self as Children>::Values>>(compiler: &mut Compiler<P>) {
                $($type::bind::<P, Field<S, <Self as Children>::Values, $index>>(compiler);)+
            }
            fn resolve(self, context: &mut ViewContext<'_>, mut id: crate::runtime::ViewId, resolution: &mut Resolution<'_>) -> Result<<Self as Children>::Values> {
                let value = ($({
                    let value = self.$index.resolve(context, id, resolution)?;
                    id.0 += $type::NODES as u64;
                    value
                },)+);
                let _ = id;
                Ok(value)
            }
        }
        impl<$($type: Body),+> Body for ($($type,)+) {
            type Values = <Self as Children>::Values;
            const SCHEMA: NodeSchema = NodeSchema {
                kind: crate::ir::Kind::Group,
                children: <Self as Children>::SCHEMAS,
            };
            const NODES: usize = 1 + <Self as Children>::NODES;
            fn bind<P: 'static, S: Select<P, Value = <Self as Body>::Values>>(compiler: &mut Compiler<P>) {
                compiler.node();
                <Self as Children>::bind::<P, S>(compiler);
            }
            fn resolve(self, context: &mut ViewContext<'_>, id: crate::runtime::ViewId, resolution: &mut Resolution<'_>) -> Result<<Self as Body>::Values> {
                <Self as Children>::resolve(self, context, crate::runtime::ViewId(id.0 + 1), resolution)
            }
        }
    };
}

macro_rules! tuples {
    (($($all:ident : $idx:tt),+);) => {};
    (($($all:ident : $idx:tt),+); $next:ident : $n:tt $(, $tail:ident : $j:tt)*) => {
        field!(($($all),+), $n, $next);
        tuples!(($($all : $idx),+); $($tail : $j),*);
    };
    ($($type:ident : $index:tt),+) => {
        tuple!($($type : $index),+);
        tuples!(($($type : $index),+); $($type : $index),+);
    };
}
tuples!(A:0);
tuples!(A:0, B:1);
tuples!(A:0, B:1, C:2);
tuples!(A:0, B:1, C:2, D:3);
tuples!(A:0, B:1, C:2, D:3, E:4);
tuples!(A:0, B:1, C:2, D:3, E:4, F:5);
tuples!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
tuples!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);

impl Children for () {
    type Values = ();
    const SCHEMAS: &'static [NodeSchema] = &[];
    const NODES: usize = 0;
    fn bind<P: 'static, S: Select<P, Value = ()>>(_: &mut Compiler<P>) {}
    fn resolve(
        self,
        _: &mut ViewContext<'_>,
        _: crate::runtime::ViewId,
        _: &mut Resolution<'_>,
    ) -> Result<()> {
        Ok(())
    }
}

impl Body for () {
    type Values = ();
    const SCHEMA: NodeSchema = NodeSchema {
        kind: crate::ir::Kind::Group,
        children: &[],
    };
    const NODES: usize = 1;
    fn bind<P: 'static, S: Select<P, Value = ()>>(compiler: &mut Compiler<P>) {
        compiler.node();
    }
    fn resolve(
        self,
        _: &mut ViewContext<'_>,
        _: crate::runtime::ViewId,
        _: &mut Resolution<'_>,
    ) -> Result<()> {
        Ok(())
    }
}
