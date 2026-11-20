use super::{Body, NodeSchema};
use crate::{
    Key,
    ir::{
        Binding, Declaration, Local, Property, Recipe, Result, Source, Structure, Template, Value,
    },
};
use std::{marker::PhantomData, rc::Rc, sync::Arc};

/// A monomorphized field path, with no closure or lookup per instance.
pub trait Select<P>: 'static {
    type Value: PartialEq + 'static;
    fn get(value: &P) -> Option<&Self::Value>;
}

pub struct First<S>(PhantomData<S>);
pub struct Second<S>(PhantomData<S>);

impl<P, S, A, B> Select<P> for First<S>
where
    S: Select<P, Value = (A, B)>,
    A: PartialEq + 'static,
    B: PartialEq + 'static,
{
    type Value = A;
    fn get(value: &P) -> Option<&A> {
        S::get(value).map(|value| &value.0)
    }
}
impl<P, S, A, B> Select<P> for Second<S>
where
    S: Select<P, Value = (A, B)>,
    A: PartialEq + 'static,
    B: PartialEq + 'static,
{
    type Value = B;
    fn get(value: &P) -> Option<&B> {
        S::get(value).map(|value| &value.1)
    }
}

struct Root;
impl<P: PartialEq + 'static> Select<Option<P>> for Root {
    type Value = P;
    fn get(value: &Option<P>) -> Option<&P> {
        value.as_ref()
    }
}

pub struct Compiler<P> {
    cursor: usize,
    recipes: Vec<Recipe>,
    sources: Vec<Source<P>>,
    groups: Vec<crate::ir::SourceGroup<P>>,
    bindings: Vec<Binding<P>>,
}

impl<P: 'static> Compiler<P> {
    pub(super) fn composite(
        &mut self,
        same_snapshot: fn(&P, &P) -> bool,
        bind: impl FnOnce(&mut Self),
    ) {
        let start = self.sources.len();
        let nested = self.groups.len();
        bind(self);
        let range = start..self.sources.len();
        // Keep disjoint leaf ranges. Modifiers outside the child register their own sources.
        if range.len() >= 2 && self.groups.len() == nested {
            self.groups.push(crate::ir::SourceGroup {
                range,
                unchanged: same_snapshot,
            });
        }
    }

    pub fn current(&self) -> Local {
        Local(self.cursor.try_into().expect("template too large"))
    }

    pub fn node(&mut self) -> Local {
        let node = Local(self.cursor.try_into().expect("template too large"));
        self.cursor += 1;
        node
    }

    pub fn recipe(&mut self, node: Local, recipe: Recipe) {
        self.recipes[node.0 as usize] = recipe;
    }

    pub fn hit_testing<S: Select<P, Value = bool>>(&mut self, start: Local, schema: NodeSchema) {
        for node in hit_roots(start, schema) {
            // The outer modifier replaces an earlier modifier on the same root.
            if let Some(index) = self.bindings.iter().position(|binding| {
                binding.node == node && binding.property == Property::HitTesting
            }) {
                self.bindings[index].evaluate =
                    Rc::new(|input, _, _| Value::HitTesting(*S::get(input).unwrap()));
                for source in &mut self.sources {
                    source.bindings = source
                        .bindings
                        .iter()
                        .copied()
                        .filter(|&value| value != index)
                        .collect();
                }
                self.sources.push(Source {
                    equal: |before, after| S::get(before) == S::get(after),
                    bindings: Box::new([index]),
                });
            } else {
                self.bind::<S>(node, Property::HitTesting, |enabled, _, _| {
                    Value::HitTesting(*enabled)
                });
            }
        }
    }

    pub(super) fn inherited_hit_testing<S: Select<P, Value = Option<bool>>>(
        &mut self,
        start: Local,
        schema: NodeSchema,
    ) {
        for node in hit_roots(start, schema) {
            let index = if let Some(index) = self.bindings.iter().position(|binding| {
                binding.node == node && binding.property == Property::HitTesting
            }) {
                let fallback = self.bindings[index].evaluate.clone();
                self.bindings[index].evaluate = Rc::new(move |input, key, revision| {
                    S::get(input)
                        .unwrap()
                        .map_or_else(|| fallback(input, key, revision), Value::HitTesting)
                });
                index
            } else {
                let index = self.bindings.len();
                self.bindings.push(Binding {
                    node,
                    property: Property::HitTesting,
                    evaluate: Rc::new(|input, _, _| {
                        Value::HitTesting(S::get(input).unwrap().unwrap_or(true))
                    }),
                });
                index
            };
            self.sources.push(Source {
                equal: |before, after| S::get(before) == S::get(after),
                bindings: Box::new([index]),
            });
        }
    }

    pub fn bind<S: Select<P>>(
        &mut self,
        node: Local,
        property: Property,
        evaluate: fn(&S::Value, Key, u64) -> Value,
    ) {
        let index = self.bindings.len();
        self.sources.push(Source {
            equal: |before, after| S::get(before) == S::get(after),
            bindings: Box::new([index]),
        });
        self.bindings.push(Binding {
            node,
            property,
            evaluate: Rc::new(move |input, key, revision| {
                evaluate(
                    S::get(input).expect("body evaluated before admission"),
                    key,
                    revision,
                )
            }),
        });
    }

    pub(super) fn override_binding<S: Select<P>>(
        &mut self,
        node: Local,
        property: Property,
        evaluate: fn(&S::Value, Key, u64) -> Value,
    ) {
        let Some(index) = self
            .bindings
            .iter()
            .position(|binding| binding.node == node && binding.property == property)
        else {
            self.bind::<S>(node, property, evaluate);
            return;
        };
        self.bindings[index].evaluate =
            Rc::new(move |input, key, revision| evaluate(S::get(input).unwrap(), key, revision));
        for source in &mut self.sources {
            source.bindings = source
                .bindings
                .iter()
                .copied()
                .filter(|&binding| binding != index)
                .collect();
        }
        self.sources.push(Source {
            equal: |before, after| S::get(before) == S::get(after),
            bindings: Box::new([index]),
        });
    }
    pub(super) fn layout_value<
        S: Select<P>,
        K: crate::layout::values::LayoutValueKey<Value = S::Value>,
    >(
        &mut self,
        node: Local,
        single: fn(&S::Value) -> Value,
    ) where
        S::Value: Clone,
    {
        let index =
            if let Some(index) = self.bindings.iter().position(|binding| {
                binding.node == node && binding.property == Property::LayoutValues
            }) {
                let previous = self.bindings[index].evaluate.clone();
                self.bindings[index].evaluate = Rc::new(move |input, key, revision| {
                    let values = match previous(input, key, revision) {
                        Value::LayoutValues(values) => values,
                        Value::Placement(bounds) => {
                            crate::layout::values::LayoutValues::default()
                                .with::<crate::layout::positioned::Placement>(bounds)
                        }
                        _ => unreachable!("layout-value binding"),
                    };
                    Value::LayoutValues(values.with::<K>(S::get(input).unwrap().clone()))
                });
                index
            } else {
                let index = self.bindings.len();
                self.bindings.push(Binding {
                    node,
                    property: Property::LayoutValues,
                    evaluate: Rc::new(move |input, _, _| single(S::get(input).unwrap())),
                });
                index
            };
        self.sources.push(Source {
            equal: |before, after| S::get(before) == S::get(after),
            bindings: Box::new([index]),
        });
    }
}

pub(super) fn hit_roots(start: Local, schema: NodeSchema) -> Vec<Local> {
    fn visit(schema: NodeSchema, cursor: &mut u32, nodes: &mut Vec<Local>, hidden: bool) {
        let node = Local(*cursor);
        *cursor += 1;
        let materialized = matches!(
            schema.kind,
            crate::ir::Kind::Content | crate::ir::Kind::Layout | crate::ir::Kind::DynamicLayout
        );
        if materialized && !hidden {
            nodes.push(node);
        }
        for &child in schema.children {
            visit(child, cursor, nodes, hidden || materialized);
        }
    }
    let mut nodes = Vec::new();
    let mut cursor = start.0;
    visit(schema, &mut cursor, &mut nodes, false);
    nodes
}

pub fn compile<B: Body>() -> Result<Template<Option<B::Values>>> {
    fn flatten(shape: NodeSchema, parent: Option<Local>, out: &mut Vec<Declaration>) {
        let local = Local(out.len() as u32);
        out.push(Declaration {
            parent,
            kind: shape.kind,
        });
        for &child in shape.children {
            flatten(child, Some(local), out);
        }
    }
    let mut declarations = Vec::with_capacity(B::NODES);
    flatten(B::SCHEMA, None, &mut declarations);
    if declarations.len() != B::NODES {
        return Err("body shape count differs from schema");
    }
    let mut compiler = Compiler {
        cursor: 0,
        recipes: (0..B::NODES).map(|_| Recipe::default()).collect(),
        sources: Vec::new(),
        groups: Vec::new(),
        bindings: Vec::new(),
    };
    B::bind::<Option<B::Values>, Root>(&mut compiler);
    if compiler.cursor != B::NODES {
        return Err("body bindings differ from schema");
    }
    Template::compile(
        Arc::new(Structure::compile(&declarations)?),
        compiler.recipes,
        compiler.sources,
        compiler.bindings,
    )
    .and_then(|template| template.with_source_groups(compiler.groups))
    .map(|template| template.with_input_equality(PartialEq::eq))
}
