use super::{
    Body, Compiler, NodeSchema, Resolution, Select, ViewContext,
    compiler::{First, Second, hit_roots},
    layout_root::LayoutRoot,
};
use crate::{
    ir::{Property, Result, Value},
    runtime::ViewId,
};
use std::marker::PhantomData;

pub(super) trait Metadata: 'static {
    type Value: PartialEq + 'static;
    const PROPERTY: Property;
    fn value(value: &Self::Value) -> Value;
    fn bind<P: 'static, S: Select<P, Value = Self::Value>>(
        compiler: &mut Compiler<P>,
        node: crate::ir::Local,
    ) {
        compiler.override_binding::<S>(node, Self::PROPERTY, |value, _, _| Self::value(value));
    }
}

pub(super) struct Role;
impl Metadata for Role {
    type Value = crate::semantics::Role;
    const PROPERTY: Property = Property::Role;
    fn value(value: &Self::Value) -> Value {
        Value::Role(*value)
    }
}

pub(super) struct WindowAction;
impl Metadata for WindowAction {
    type Value = Option<crate::semantics::WindowAction>;
    const PROPERTY: Property = Property::WindowAction;
    fn value(value: &Self::Value) -> Value {
        Value::WindowAction(*value)
    }
}

pub(super) struct LayoutValue<K>(PhantomData<K>);
impl<K: crate::layout::values::LayoutValueKey> Metadata for LayoutValue<K> {
    type Value = K::Value;
    const PROPERTY: Property = Property::LayoutValues;
    fn value(value: &Self::Value) -> Value {
        Value::LayoutValues(crate::layout::values::LayoutValues::default().with::<K>(value.clone()))
    }
    fn bind<P: 'static, S: Select<P, Value = Self::Value>>(
        compiler: &mut Compiler<P>,
        node: crate::ir::Local,
    ) {
        compiler.layout_value::<S, K>(node, Self::value);
    }
}

pub(super) struct Placed;
impl Metadata for Placed {
    type Value = Option<crate::Bounds>;
    const PROPERTY: Property = Property::LayoutValues;
    fn value(value: &Self::Value) -> Value {
        Value::Placement(*value)
    }
    fn bind<P: 'static, S: Select<P, Value = Self::Value>>(
        compiler: &mut Compiler<P>,
        node: crate::ir::Local,
    ) {
        compiler.layout_value::<S, crate::layout::positioned::Placement>(node, Self::value);
    }
}

pub(super) struct Clip;
impl Metadata for Clip {
    type Value = bool;
    const PROPERTY: Property = Property::Clip;
    fn value(value: &bool) -> Value {
        Value::Clip(*value)
    }
}
pub(super) struct Interactive;
impl Metadata for Interactive {
    type Value = bool;
    const PROPERTY: Property = Property::Interactive;
    fn value(value: &bool) -> Value {
        Value::Interactive(*value)
    }
    fn bind<P: 'static, S: Select<P, Value = bool>>(
        compiler: &mut Compiler<P>,
        node: crate::ir::Local,
    ) {
        compiler.override_binding::<S>(node, Self::PROPERTY, |value, _, _| Self::value(value));
        compiler.override_binding::<S>(node, Property::Input, |enabled, _, _| Value::Input {
            enabled: *enabled,
            options: Some(crate::input::InputOptions {
                activation: crate::input::Activation::Release,
                ..Default::default()
            }),
        });
    }
}
pub(super) struct Reuse;
impl Metadata for Reuse {
    type Value = retinue_render::CacheHint;
    const PROPERTY: Property = Property::Reuse;
    fn value(value: &Self::Value) -> Value {
        Value::Reuse(*value)
    }
}

pub(super) fn attach<C: Body, M: Metadata>(content: C, value: M::Value) -> impl Body {
    Attached::<C, M> {
        content,
        value,
        marker: PhantomData,
    }
}

struct Attached<C, M: Metadata> {
    content: C,
    value: M::Value,
    marker: PhantomData<M>,
}
impl<C: Body, M: Metadata> Body for Attached<C, M> {
    type Values = (M::Value, C::Values);
    const SCHEMA: NodeSchema = LayoutRoot::<C>::SCHEMA;
    const NODES: usize = LayoutRoot::<C>::NODES;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.current();
        LayoutRoot::<C>::bind::<P, Second<S>>(compiler);
        let roots = hit_roots(node, Self::SCHEMA);
        assert_eq!(roots.len(), 1);
        M::bind::<P, First<S>>(compiler, roots[0]);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((
            self.value,
            LayoutRoot(self.content).resolve(context, id, resolution)?,
        ))
    }
}
