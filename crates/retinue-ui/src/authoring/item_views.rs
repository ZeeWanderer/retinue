use super::{Body, Compiler, NodeSchema, Resolution, Select, View, ViewContext, resolve::Scoped};
use crate::{
    Key,
    environment::Environment,
    ir::{Kind, Local, Result},
    runtime::ViewId,
    state::{Items, KeyedOrder, State},
    view::Owned,
};
use std::{cell::RefCell, rc::Rc};

/// A keyed child whose input and lifetime can be tracked independently of the collection.
pub trait ItemView: View {
    type Item: 'static;
    fn from_item(key: Key, item: State<Self::Item>) -> Self;
}

/// Bind each row to its own observation owner; reordering does not reevaluate retained rows.
pub struct ItemViews<V: ItemView>(pub Items<V::Item>);

struct Window<T: 'static> {
    items: Items<T>,
    order: KeyedOrder,
    environment: Environment,
    hit_testing: Option<bool>,
    discard: u64,
}

impl<V: ItemView> Body for ItemViews<V> {
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
        r: &mut Resolution<'_>,
    ) -> Result<()> {
        let previous = context.scoped(id, |c| {
            c.owned_property::<Self, _>("window", || Rc::new(RefCell::new(None)))
        });
        self.update(context, id, r, &previous, |view, c| view.body(c))
    }
}

impl<V: ItemView> ItemViews<V> {
    fn update<B: Body>(
        self,
        c: &mut ViewContext<'_>,
        id: ViewId,
        r: &mut Resolution<'_>,
        previous: &RefCell<Option<Window<V::Item>>>,
        body: impl Fn(&V, &mut ViewContext<'_>) -> B + Copy + 'static,
    ) -> Result<()> {
        let order = self.0.order();
        let mut previous = previous.borrow_mut();
        let retained = previous.as_ref().filter(|w| {
            w.items == self.0
                && w.environment == *c.environment
                && w.hit_testing == r.hit_testing
                && w.discard == r.graph.discard_revision
        });
        if retained.is_some_and(|w| w.order == order) {
            return Ok(());
        }
        let program = r.graph.body_template::<Scoped<B>>()?;
        let handles = r.graph.instances(
            r.owner,
            Local((id.0 - r.base.0) as u32),
            order.keys(),
            program,
            || None,
        )?;
        for (&key, handle) in order.keys().iter().zip(handles) {
            if retained.is_some_and(|w| w.order.index(key).is_some()) {
                continue;
            }
            let item = self.0.item(key).ok_or("missing keyed row")?;
            let hit_testing = r.hit_testing;
            let mut initial_mutation = c.mutation;
            r.graph
                .observe_child(handle, c.environment.clone(), move |c, graph| {
                    let base = c.id();
                    graph.with_state::<Owned, _>(handle, |graph, owned| {
                        let mut c = ViewContext {
                            owned,
                            mutation: c.mutation.or(initial_mutation.take()),
                            ..c.reborrow()
                        };
                        let content = {
                            #[cfg(feature = "profiling")]
                            let _scope =
                                crate::profiling::Scope::enter(crate::profiling::Phase::Body);
                            body(&V::from_item(key, item.clone()), &mut c)
                        };
                        Scoped {
                            hit_testing,
                            body: content,
                        }
                        .resolve(
                            &mut c,
                            base,
                            &mut Resolution {
                                graph,
                                owner: handle,
                                base,
                                hit_testing,
                            },
                        )
                        .map(Some)
                    })
                })?;
        }
        *previous = Some(Window {
            items: self.0,
            order,
            environment: c.environment.clone(),
            hit_testing: r.hit_testing,
            discard: r.graph.discard_revision,
        });
        Ok(())
    }
}
