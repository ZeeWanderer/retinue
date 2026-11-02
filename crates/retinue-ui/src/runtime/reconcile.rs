use super::{
    ViewId, ViewTree,
    node::{Body, Node},
};
use crate::{
    environment::Environment,
    state::{Mutation, Observation, with_transaction},
    view::{Element, Kind, Owned, ViewContext},
};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::{cell::RefCell, rc::Rc};

impl ViewTree {
    pub(super) fn compatible(&self, id: ViewId, input: &Element) -> bool {
        let node = self.nodes[&id].borrow();
        node.input.key == input.key && node.input.same_type(input)
    }

    pub(super) fn mount(
        &mut self,
        input: Element,
        parent: Option<ViewId>,
        environment: Environment,
        cause: Option<Mutation>,
    ) -> ViewId {
        let id = ViewId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("view identity exhausted");
        let depth = parent.map_or(0, |parent| self.nodes[&parent].borrow().depth + 1);
        if let Some(key) = input.key {
            self.names.entry(key).or_default().push(id);
        }
        if input.input.is_some() {
            self.input_nodes.insert(id);
            self.input_dirty = true;
        }
        let body = matches!(&*input.kind, Kind::View(_) | Kind::Primitive(_)).then(|| {
            let queue = Rc::downgrade(&self.queue);
            Body {
                owned: Owned::default(),
                deadlines: Default::default(),
                observation: Observation::new(move || {
                    if let Some(queue) = queue.upgrade() {
                        queue.insert(depth, id);
                    }
                }),
            }
        });
        let environment = if matches!(&*input.kind, Kind::Environment(..)) {
            environment.child()
        } else {
            environment
        };
        self.nodes.insert(
            id,
            Rc::new(RefCell::new(Node {
                parent,
                depth,
                input,
                applied: None,
                children: Vec::new(),
                environment,
                body,
                primitive: None,
                revision: 0,
                cause,
                last_mutation: None,
                retry_body: false,
                timed_update: false,
                animation_scope: None,
            })),
        );
        self.queue.insert(depth, id);
        self.stats.mounted += 1;
        id
    }

    pub(super) fn unmount(&mut self, id: ViewId) {
        self.timing.remove(id);
        self.presentations.remove(&id);
        let node = self.nodes.remove(&id).unwrap();
        let node = node.borrow();
        if let Some(key) = node.input.key {
            if let Some(ids) = self.names.get_mut(&key) {
                ids.retain(|current| *current != id);
                if ids.is_empty() {
                    self.names.remove(&key);
                }
            }
        }
        self.queue.remove(node.depth, id);
        self.changed.remove(&id);
        self.structural.remove(&id);
        self.input_dirty |= self.input_nodes.remove(&id);
        self.stats.unmounted += 1;
        for &child in &node.children {
            self.unmount(child);
        }
    }

    pub(super) fn set_input(&mut self, id: ViewId, input: Element, cause: Option<Mutation>) {
        let mut node = self.nodes[&id].borrow_mut();
        if node.input != input {
            self.input_dirty |= node.input.input != input.input;
            if input.input.is_some() {
                self.input_nodes.insert(id);
            } else {
                self.input_nodes.remove(&id);
            }
            node.input = input;
            node.cause = Mutation::latest(node.cause, cause);
            self.queue.insert(node.depth, id);
        }
    }

    pub(super) fn refresh(&mut self, id: ViewId) -> Result<(), &'static str> {
        let retained = self.nodes[&id].clone();
        let inherited = self.scoped_mutation(id, None);
        let (children, cause, output_changed, before, after) = {
            let mut node = retained.borrow_mut();
            let input_changed = node.applied.as_ref() != Some(&node.input);
            let dirty = node
                .body
                .as_ref()
                .is_some_and(|body| body.observation.is_dirty());
            if !input_changed && !dirty && !node.retry_body && !node.timed_update {
                node.cause = None;
                return Ok(());
            }
            let mut cause = Mutation::latest(
                node.cause,
                node.body
                    .as_ref()
                    .and_then(|body| body.observation.pending_mutation()),
            );
            if let Some(scope) = inherited {
                let mut scoped = cause.unwrap_or(scope);
                let disabled = scoped.transaction.disables_animations;
                scoped.transaction = scope.transaction;
                scoped.transaction.disables_animations |= disabled;
                cause = Some(scoped);
            }
            if let Some(scope) = &node.input.animation {
                if node.applied.as_ref().is_some_and(|old| {
                    old.animation
                        .as_ref()
                        .is_none_or(|old| !scope.same_trigger(old))
                }) {
                    let mut scoped = cause.unwrap_or_else(Mutation::capture);
                    scoped.transaction = scope.apply(scoped.transaction);
                    node.animation_scope = Some((self.epoch, scoped.transaction));
                    cause = Some(scoped);
                }
            }
            node.cause = cause;
            let input = node.input.clone();
            let before = node.applied.as_ref().and_then(|input| {
                crate::motion::presentation::Visual::of(input, node.primitive.as_ref())
            });
            let values_changed = node
                .applied
                .as_ref()
                .is_none_or(|old| old.layout_values != input.layout_values);
            let mut output_changed = node.applied.as_ref().is_none_or(|old| {
                !old.kind.same_output(&input.kind) || old.attributes != input.attributes
            }) || values_changed;
            let environment = node.environment.clone();
            let children = match &*input.kind {
                Kind::Primitive(primitive) => {
                    node.retry_body = true;
                    let body = node.body.as_mut().unwrap();
                    let mut deadlines = crate::timeline::Deadlines::default();
                    let output = body.observation.track(|| {
                        primitive.resolve(&mut ViewContext {
                            id,
                            environment: &environment,
                            owned: &mut body.owned,
                            mutation: cause,
                            time: self.time,
                            deadlines: &mut deadlines,
                            frame_interval: self.frame_interval,
                            motion_enabled: self.motion_enabled,
                        })
                    });
                    self.stats.invalid_deadlines += deadlines.invalid;
                    body.deadlines = deadlines;
                    output_changed = node.primitive.as_ref() != Some(&output)
                        || values_changed
                        || node
                            .applied
                            .as_ref()
                            .is_none_or(|old| old.attributes != input.attributes);
                    node.primitive = Some(output);
                    Vec::new()
                }
                Kind::View(view) => {
                    node.retry_body = true;
                    let body = node.body.as_mut().unwrap();
                    self.stats.bodies += 1;
                    let mut deadlines = crate::timeline::Deadlines::default();
                    let child = body.observation.track(|| {
                        view.body(&mut ViewContext {
                            id,
                            environment: &environment,
                            owned: &mut body.owned,
                            mutation: cause,
                            time: self.time,
                            deadlines: &mut deadlines,
                            frame_interval: self.frame_interval,
                            motion_enabled: self.motion_enabled,
                        })
                    });
                    self.stats.invalid_deadlines += deadlines.invalid;
                    body.deadlines = deadlines;
                    vec![child]
                }
                Kind::Layout(layout) => layout.children.clone(),
                Kind::Fragment(children) => children.clone(),
                Kind::Environment(provider, child) => {
                    with_transaction(
                        cause.map_or_else(Default::default, |cause| cause.transaction),
                        || provider.apply(&environment),
                    );
                    vec![child.clone()]
                }
                Kind::Rectangle(_) | Kind::Empty => Vec::new(),
            };
            let mut keys = HashSet::default();
            if children
                .iter()
                .filter_map(|child| child.key)
                .any(|key| !keys.insert(key))
            {
                return Err("duplicate sibling view identity");
            }
            node.applied = Some(input);
            node.retry_body = false;
            node.timed_update = false;
            node.cause = None;
            node.last_mutation = cause;
            if let Some(body) = &node.body {
                self.timing.replace(id, &body.deadlines);
            }
            if input_changed || output_changed {
                node.revision += 1;
            }
            let after =
                crate::motion::presentation::Visual::of(&node.input, node.primitive.as_ref());
            (children, cause, output_changed, before, after)
        };
        self.retarget_presentation(id, before, after, cause);
        self.reconcile_children(id, children, cause);
        if output_changed {
            self.changed.insert(id);
        }
        Ok(())
    }

    fn reconcile_children(
        &mut self,
        parent: ViewId,
        inputs: Vec<Element>,
        cause: Option<Mutation>,
    ) {
        let (old, environment) = {
            let node = self.nodes[&parent].borrow();
            (node.children.clone(), node.environment.clone())
        };
        let keyed: HashMap<_, _> = old
            .iter()
            .filter_map(|id| self.nodes[id].borrow().input.key.map(|key| (key, *id)))
            .collect();
        let mut kept = HashSet::default();
        let mut children = Vec::with_capacity(inputs.len());
        for (index, input) in inputs.into_iter().enumerate() {
            let candidate = match input.key {
                Some(key) => keyed.get(&key),
                None => old.get(index),
            };
            let id = if let Some(&id) = candidate.filter(|id| self.compatible(**id, &input)) {
                self.set_input(id, input, cause);
                id
            } else {
                self.mount(input, Some(parent), environment.clone(), cause)
            };
            kept.insert(id);
            children.push(id);
        }
        if old != children {
            self.changed.insert(parent);
            self.structural.insert(parent);
        }
        self.nodes[&parent].borrow_mut().children = children;
        for id in old {
            if !kept.contains(&id) {
                self.unmount(id);
            }
        }
    }
}
