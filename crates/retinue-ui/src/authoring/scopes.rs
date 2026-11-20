use super::{
    Body, Compiler, NodeSchema, Resolution, Select, ViewContext,
    compiler::{First, Second},
};
use crate::{
    environment::Environment,
    input::InputOptions,
    ir::{Kind, Local, Property, Result, Value},
    runtime::ViewId,
};

pub(super) fn environment<C: Body>(
    content: C,
    provide: impl FnOnce(&Environment) + 'static,
) -> impl Body {
    EnvironmentScope { content, provide }
}

struct EnvironmentScope<C, F> {
    content: C,
    provide: F,
}

impl<C: Body, F: FnOnce(&Environment) + 'static> Body for EnvironmentScope<C, F> {
    type Values = C::Values;
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Group,
        children: &[C::SCHEMA],
    };
    const NODES: usize = 1 + C::NODES;
    const BACKGROUND_TARGET: Option<Local> = match C::BACKGROUND_TARGET {
        Some(Local(index)) => Some(Local(index + 1)),
        None => None,
    };

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
        context.scoped(id, |context| {
            let parent = context.environment;
            let environment = context.owned_property::<Self, _>("environment", || parent.child());
            (self.provide)(&environment);
            context.with_scope(id, &environment, |context| {
                self.content.resolve(context, ViewId(id.0 + 1), resolution)
            })
        })
    }
}

pub struct InputScope<C> {
    content: C,
    options: InputOptions,
    enabled: bool,
}

impl<C> InputScope<C> {
    pub(super) fn new(content: C, options: InputOptions) -> Self {
        Self {
            content,
            options,
            enabled: true,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<C: Body> Body for InputScope<C> {
    type Values = ((bool, InputOptions), C::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Group,
        children: &[C::SCHEMA],
    };
    const NODES: usize = 1 + C::NODES;
    const BACKGROUND_TARGET: Option<Local> = match C::BACKGROUND_TARGET {
        Some(Local(index)) => Some(Local(index + 1)),
        None => None,
    };

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<First<S>>(node, Property::Input, |(enabled, options), _, _| {
            Value::Input {
                enabled: *enabled,
                options: Some(options.clone()),
            }
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
            (self.enabled, self.options),
            self.content
                .resolve(context, ViewId(id.0 + 1), resolution)?,
        ))
    }
}
