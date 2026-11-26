use super::edits::Edits;
use super::owner::Clock;
use super::{
    Bank, Kind, Local, Result, Template, Work,
    store::{Handle, Pool},
};
use crate::{
    Key,
    scene::{self, LayoutExecution, RecordingMode, Scene, layout::Subtree},
    timeline::{FrameTime, Timing},
};
use retinue_render::damage::Damage;
use rustc_hash::{FxHashMap as Map, FxHashSet as Set};
use std::{
    any::TypeId,
    collections::BTreeMap,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    task::Poll,
};

static NEXT_GRAPH: AtomicU64 = AtomicU64::new(1);

mod input;
mod names;
mod observation;
mod state;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Address {
    pub instance: Handle,
    pub local: Local,
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Port {
    owner: Handle,
    local: Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Child {
    pub key: Key,
    pub handle: Handle,
}

struct Topology {
    root: Option<Handle>,
    active: Set<Handle>,
    affected: Set<Handle>,
    parent_changes: Vec<(Handle, Option<Address>)>,
}

pub struct Update {
    /// Latest accepted generation, including work not yet published.
    pub accepted: u64,
    /// Generation currently exposed by `scene`, `address` and `geometry`.
    pub published: u64,
    pub work: Work,
    pub completion: Result<Poll<Damage>>,
}

/// Owns a mounted view hierarchy and updates its layout and drawing when inputs change.
///
/// A host creates a graph with [`Self::with_root_view`], retains it between frames,
/// and supplies the available size and scale when updating it.
pub struct InstanceGraph {
    #[cfg(feature = "profiling")]
    pub profile: crate::runtime::UpdateProfile,
    scene: Scene,
    host: Key,
    banks: Map<u64, Box<dyn Pool>>,
    types: Map<TypeId, u64>,
    pub(crate) templates: Map<TypeId, Rc<dyn std::any::Any>>,
    ranges: BTreeMap<u64, (u64, Handle)>,
    dirty: Set<u64>,
    changes: Edits,
    observations: Option<Rc<observation::Queue>>,
    clock: Option<Clock>,
    timing: Timing<Handle>,
    inputs: input::Inputs,
    host_routes_input: bool,
    names: names::Names,
    root: Option<Handle>,
    pending_root: Option<Option<Handle>>,
    links: Map<Port, Vec<Child>>,
    pending_links: Map<Port, Vec<Child>>,
    topology_revision: u64,
    parents: Map<Handle, Address>,
    unpublished_parents: Vec<(Handle, Option<Address>)>,
    active: Set<Handle>,
    visible: Set<Handle>,
    born: Set<Handle>,
    retired: Set<Handle>,
    generation: u64,
    published: u64,
    topology_unpublished: bool,
    failed: bool,
    pub(crate) discard_revision: u64,
}

impl Default for InstanceGraph {
    fn default() -> Self {
        let host = Key::new("ui.ir.host", NEXT_GRAPH.fetch_add(1, Ordering::Relaxed));
        let mut scene = Scene::default();
        scene
            .update(
                Self::host_element(host),
                (1, 1),
                1.0,
                RecordingMode::Retained,
            )
            .unwrap();
        Self {
            #[cfg(feature = "profiling")]
            profile: crate::runtime::UpdateProfile::default(),
            scene,
            host,
            banks: Map::default(),
            types: Map::default(),
            templates: Map::default(),
            ranges: BTreeMap::new(),
            dirty: Set::default(),
            changes: Edits::default(),
            observations: None,
            clock: None,
            timing: Timing::default(),
            inputs: input::Inputs::default(),
            host_routes_input: false,
            names: names::Names::default(),
            root: None,
            pending_root: None,
            links: Map::default(),
            pending_links: Map::default(),
            topology_revision: 0,
            parents: Map::default(),
            unpublished_parents: Vec::new(),
            active: Set::default(),
            visible: Set::default(),
            born: Set::default(),
            retired: Set::default(),
            generation: 0,
            published: 0,
            topology_unpublished: false,
            failed: false,
            discard_revision: 0,
        }
    }
}

impl InstanceGraph {
    /// Publish scene geometry, but leave input dispatch (including focus and
    /// capture) to the host. Authored handlers are not installed in this mode.
    pub fn with_host_routed_input() -> Self {
        Self {
            host_routes_input: true,
            ..Self::default()
        }
    }

    fn host_element(key: Key) -> scene::Element {
        let mut root = scene::Element::new(
            key,
            taffy::Style {
                align_items: Some(taffy::AlignItems::START),
                size: taffy::Size {
                    width: taffy::Dimension::percent(1.0),
                    height: taffy::Dimension::percent(1.0),
                },
                ..Default::default()
            },
        );
        root.algorithm = scene::Algorithm::Custom(crate::layout::LayoutAlgorithm::new(
            crate::view::stack_layout::StackLayout {
                alignment: crate::view::Alignment::Start,
            },
        ));
        root
    }

    fn bank<P: 'static>(&mut self) -> &mut Bank<P> {
        let id = *self.types.entry(TypeId::of::<P>()).or_insert_with(|| {
            let bank = Bank::<P>::default();
            let id = bank.id();
            self.banks.insert(id, Box::new(bank));
            id
        });
        self.banks
            .get_mut(&id)
            .unwrap()
            .as_any_mut()
            .downcast_mut()
            .unwrap()
    }

    fn pool(&self, handle: Handle) -> Result<&dyn Pool> {
        self.banks
            .get(&handle.bank)
            .map(|pool| &**pool)
            .ok_or("instance belongs to another graph")
    }

    pub fn mount<P: 'static>(&mut self, program: Rc<Template<P>>, input: P) -> Handle {
        let handle = self.bank::<P>().mount(program, input);
        let extent = self.pool(handle).unwrap().extent(handle).unwrap();
        self.ranges.insert(extent.start, (extent.end, handle));
        self.born.insert(handle);
        handle
    }

    pub fn stage<P: 'static>(&mut self, handle: Handle, input: P) -> Result<()> {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Stage);
        let pool = self
            .banks
            .get_mut(&handle.bank)
            .ok_or("instance belongs to another graph")?;
        let staged = pool
            .as_any_mut()
            .downcast_mut::<Bank<P>>()
            .ok_or("input schema differs from instance")?
            .stage(handle, input)?;
        if staged {
            self.dirty.insert(handle.bank);
        }
        Ok(())
    }

    pub fn set_root(&mut self, root: Option<Handle>) -> Result<()> {
        if let Some(handle) = root {
            self.key(handle, Local(0))?;
        }
        self.pending_root = Some(root);
        self.topology_revision += 1;
        Ok(())
    }

    fn port(&self, owner: Handle, local: Local) -> Result<Port> {
        if !self.pool(owner)?.outlets(owner)?.contains(&local) {
            return Err("dynamic children require an outlet");
        }
        Ok(Port { owner, local })
    }

    pub fn children(
        &mut self,
        owner: Handle,
        local: Local,
        children: Vec<(Key, Handle)>,
    ) -> Result<()> {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Keys);
        let port = self.port(owner, local)?;
        let mut keys = Set::default();
        for &(key, handle) in &children {
            if !keys.insert(key) {
                return Err("duplicate collection key");
            }
            self.key(handle, Local(0))?;
        }
        self.link(
            port,
            children
                .into_iter()
                .map(|(key, handle)| Child { key, handle })
                .collect(),
        );
        Ok(())
    }

    fn link(&mut self, port: Port, children: Vec<Child>) {
        if self.children_at(port) != children {
            self.pending_links.insert(port, children);
            self.topology_revision += 1;
        }
    }

    pub(crate) fn instances<P: 'static>(
        &mut self,
        owner: Handle,
        local: Local,
        keys: &[Key],
        program: Rc<Template<P>>,
        initial: fn() -> P,
    ) -> Result<Vec<Handle>> {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Keys);
        let port = self.port(owner, local)?;
        let bank = self.types.get(&TypeId::of::<P>()).copied();
        let previous = self.children_at(port);
        if previous.len() == keys.len()
            && previous
                .iter()
                .zip(keys)
                .all(|(child, key)| child.key == *key)
            && bank.is_some_and(|bank| previous.iter().all(|child| child.handle.bank == bank))
        {
            let handles: Vec<_> = previous.iter().map(|child| child.handle).collect();
            let bank = self.bank::<P>();
            if handles.iter().all(|handle| bank.matches(*handle, &program)) {
                return Ok(handles);
            }
        }
        let mut unique = Set::default();
        if keys.iter().any(|key| !unique.insert(*key)) {
            return Err("duplicate collection key");
        }
        let previous = self.candidates(port);
        let mut children = Vec::with_capacity(keys.len());
        for &key in keys {
            let retained = previous
                .get(&key)
                .into_iter()
                .flatten()
                .flatten()
                .copied()
                .find(|handle| self.bank::<P>().matches(*handle, &program));
            let handle = retained.unwrap_or_else(|| self.mount(program.clone(), initial()));
            children.push(Child { key, handle });
        }
        let handles = children.iter().map(|child| child.handle).collect();
        self.link(port, children);
        Ok(handles)
    }

    /// Stage keyed children and borrow their proposed order without copying it.
    pub fn keyed<P: 'static>(
        &mut self,
        owner: Handle,
        local: Local,
        items: Vec<(Key, Rc<Template<P>>, P)>,
    ) -> Result<&[Child]> {
        let port = self.port(owner, local)?;
        let previous = self
            .pending_links
            .get(&port)
            .or_else(|| self.links.get(&port));
        if let Some(previous) = previous
            && previous.len() == items.len()
            && let Some(&id) = self.types.get(&TypeId::of::<P>())
        {
            let bank = self
                .banks
                .get_mut(&id)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<Bank<P>>()
                .unwrap();
            if previous
                .iter()
                .zip(&items)
                .all(|(child, (key, program, _))| {
                    child.key == *key && bank.matches(child.handle, program)
                })
            {
                let mut staged = false;
                for (child, (_, _, input)) in previous.iter().zip(items) {
                    staged |= bank.stage(child.handle, input)?;
                }
                if staged {
                    self.dirty.insert(id);
                }
                return Ok(self.children_at(port));
            }
        }
        let mut keys = Set::default();
        for (key, _, _) in &items {
            if !keys.insert(*key) {
                return Err("duplicate collection key");
            }
        }
        let previous = self.candidates(port);
        let mut children = Vec::with_capacity(items.len());
        for (key, program, input) in items {
            let retained = previous
                .get(&key)
                .into_iter()
                .flatten()
                .flatten()
                .copied()
                .find(|handle| self.bank::<P>().matches(*handle, &program));
            let handle = if let Some(handle) = retained {
                self.stage(handle, input)?;
                handle
            } else {
                self.mount(program, input)
            };
            children.push(Child { key, handle });
        }
        self.link(port, children);
        Ok(self.children_at(port))
    }

    pub fn key(&self, handle: Handle, local: Local) -> Result<Key> {
        self.pool(handle)?.key(handle, local)
    }
    /// Sparse owner-affine state, retained through the final visible generation.
    pub fn state<T: Default + 'static>(&mut self, handle: Handle) -> Result<&mut T> {
        self.banks
            .get_mut(&handle.bank)
            .ok_or("instance belongs to another graph")?
            .state(handle, || Box::<T>::default())?
            .downcast_mut()
            .ok_or("owner state type changed")
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn frame(&self) -> &retinue_render::PaintFrame {
        self.scene.frame()
    }

    pub fn named_geometry(&self) -> impl Iterator<Item = scene::Geometry> + '_ {
        self.scene.geometry().iter().filter_map(|geometry| {
            Some(scene::Geometry {
                key: self.authored_key(geometry.key)?,
                parent: geometry.parent.and_then(|key| self.authored_key(key)),
                ..*geometry
            })
        })
    }
    pub fn is_layout_pending(&self) -> bool {
        self.scene.pending_layout().is_some()
    }
    pub fn contains(&self, handle: Handle) -> bool {
        self.key(handle, Local(0)).is_ok()
    }
    pub fn is_visible(&self, handle: Handle) -> bool {
        self.visible.contains(&handle)
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn address(&self, key: Key) -> Option<Address> {
        self.locate(key)
            .filter(|address| self.is_visible(address.instance))
    }

    /// Named geometry from the published generation, never pending layout inputs.
    pub fn unique(&self, name: Key) -> Result<Option<Address>> {
        Ok(self.named_bounds(name)?.and_then(|g| self.address(g.key)))
    }

    /// Query published geometry without resolving a logical instance address.
    pub fn named_bounds(&self, name: Key) -> Result<Option<scene::Geometry>> {
        let mut visible = self
            .names
            .keys(name)
            .iter()
            .filter_map(|&k| self.scene.bounds(k));
        let first = visible.next();
        if visible.next().is_some() {
            return Err("ambiguous authored name");
        }
        Ok(first)
    }

    pub fn has_name(&self, name: Key) -> bool {
        self.names
            .keys(name)
            .iter()
            .any(|&k| self.scene.bounds(k).is_some())
    }

    pub fn authored_key(&self, key: Key) -> Option<Key> {
        self.names
            .name(key)
            .filter(|_| self.scene.bounds(key).is_some())
    }

    /// Resolve a physical input/semantic target to its nearest named container.
    pub fn enclosing_name(&self, mut key: Key) -> Option<Key> {
        loop {
            if let Some(name) = self.authored_key(key) {
                return Some(name);
            }
            key = self.scene.bounds(key)?.parent?;
        }
    }

    fn locate(&self, key: Key) -> Option<Address> {
        if key.namespace != "ui.ir.node" {
            return None;
        }
        let (&base, &(end, instance)) = self.ranges.range(..=key.value).next_back()?;
        (key.value < end).then_some(Address {
            instance,
            local: Local((key.value - base) as u32),
        })
    }

    pub fn geometry(&self, address: Address) -> Option<scene::Geometry> {
        if !self.is_visible(address.instance) {
            return None;
        }
        let pool = self.pool(address.instance).ok()?;
        let key = pool.key(address.instance, address.local).ok()?;
        self.scene.bounds(key)
    }

    /// Logical parent in the published hierarchy, including transparent nodes.
    pub fn parent(&self, address: Address) -> Option<Address> {
        if !self.is_visible(address.instance) {
            return None;
        }
        let structure = self
            .pool(address.instance)
            .ok()?
            .structure(address.instance)
            .ok()?;
        let node = structure.nodes.get(address.local.0 as usize)?;
        node.parent
            .map(|local| Address {
                instance: address.instance,
                local,
            })
            .or_else(|| self.parents.get(&address.instance).copied())
    }

    fn children_at(&self, port: Port) -> &[Child] {
        self.pending_links
            .get(&port)
            .or_else(|| self.links.get(&port))
            .map_or(&[], Vec::as_slice)
    }

    fn candidates(&self, port: Port) -> Map<Key, [Option<Handle>; 2]> {
        let mut previous = Map::<Key, [Option<Handle>; 2]>::default();
        for (index, collection) in [self.pending_links.get(&port), self.links.get(&port)]
            .into_iter()
            .enumerate()
        {
            for child in collection.into_iter().flatten() {
                previous.entry(child.key).or_default()[index] = Some(child.handle);
            }
        }
        previous
    }

    fn topology_changed(&self) -> bool {
        self.pending_root.is_some_and(|root| root != self.root)
            || self.pending_links.iter().any(|(port, children)| {
                self.active.contains(&port.owner)
                    && self.links.get(port).map_or(&[][..], Vec::as_slice) != children
            })
    }

    fn topology(&self) -> Result<Topology> {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Topology);
        fn visit(
            graph: &InstanceGraph,
            handle: Handle,
            parent: Option<Address>,
            plan: &mut Topology,
        ) -> Result<bool> {
            if !plan.active.insert(handle) {
                return Err("instance has multiple parents or a cycle");
            }
            graph.key(handle, Local(0))?;
            if graph.parents.get(&handle).copied() != parent {
                plan.parent_changes.push((handle, parent));
            }
            let mut changed = !graph.active.contains(&handle);
            for &local in graph.pool(handle)?.outlets(handle)? {
                let port = Port {
                    owner: handle,
                    local,
                };
                let children = graph.children_at(port);
                changed |= graph.links.get(&port).map_or(&[][..], Vec::as_slice) != children;
                for child in children {
                    changed |= visit(
                        graph,
                        child.handle,
                        Some(Address {
                            instance: handle,
                            local,
                        }),
                        plan,
                    )?;
                }
            }
            if changed {
                plan.affected.insert(handle);
            }
            Ok(changed)
        }
        let mut plan = Topology {
            root: self.pending_root.unwrap_or(self.root),
            active: Set::default(),
            affected: Set::default(),
            parent_changes: Vec::new(),
        };
        if let Some(root) = plan.root {
            visit(self, root, None, &mut plan)?;
        }
        for handle in self.parents.keys() {
            if !plan.active.contains(handle) {
                plan.parent_changes.push((*handle, None));
            }
        }
        Ok(plan)
    }

    fn lower(
        &self,
        handle: Handle,
        plan: &Topology,
        work: &mut Work,
        roots: &mut Vec<Subtree>,
        edits: &mut Edits,
    ) -> Result<()> {
        let pool = self.pool(handle)?;
        if !plan.affected.contains(&handle) {
            let structure = pool.structure(handle)?;
            for &local in structure.roots() {
                if structure.nodes[local.0 as usize].kind == Kind::Slot {
                    for child in self.children_at(Port {
                        owner: handle,
                        local,
                    }) {
                        self.lower(child.handle, plan, work, roots, edits)?;
                    }
                } else {
                    roots.push(Subtree::Retained(pool.key(handle, local)?));
                }
            }
            return Ok(());
        }
        let nodes = pool.structure(handle)?.materialized;
        work.nodes_materialized += nodes;
        #[cfg(feature = "profiling")]
        if self.active.contains(&handle) {
            work.existing_nodes_materialized += nodes;
        }
        let mut outlets = Vec::new();
        for &local in pool.outlets(handle)? {
            let mut children = Vec::new();
            for child in self.children_at(Port {
                owner: handle,
                local,
            }) {
                self.lower(child.handle, plan, work, &mut children, edits)?;
            }
            outlets.push((local, children));
        }
        pool.project(
            handle,
            outlets,
            !self.active.contains(&handle),
            roots,
            edits,
        )
    }

    fn has_effects(&self) -> bool {
        self.topology_changed()
            || self
                .dirty
                .iter()
                .any(|id| self.banks[id].changed(&self.active))
    }

    fn collect_retired(&mut self) {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Retire);
        let dead: Vec<_> = self
            .retired
            .iter()
            .filter(|handle| !self.active.contains(handle) && !self.visible.contains(handle))
            .copied()
            .collect();
        if !dead.is_empty() {
            self.inputs.retain(&self.active, &self.visible);
            self.links.retain(|port, _| {
                !self.retired.contains(&port.owner)
                    || self.active.contains(&port.owner)
                    || self.visible.contains(&port.owner)
            });
        }
        for handle in dead {
            self.timing.remove(handle);
            self.ranges
                .remove(&self.pool(handle).unwrap().extent(handle).unwrap().start);
            self.banks
                .get_mut(&handle.bank)
                .unwrap()
                .remove(handle)
                .unwrap();
            self.retired.remove(&handle);
        }
        self.retired.retain(|handle| !self.active.contains(handle));
    }

    fn publish(&mut self) {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Publish);
        self.published = self.generation;
        self.names.publish(&self.scene, self.topology_unpublished);
        if self.topology_unpublished {
            self.inputs.topology_changed();
            for (handle, parent) in self.unpublished_parents.drain(..) {
                if let Some(parent) = parent {
                    self.parents.insert(handle, parent);
                } else {
                    self.parents.remove(&handle);
                }
            }
            self.visible.clone_from(&self.active);
            self.topology_unpublished = false;
        }
    }

    /// Discard staged inputs and identities, not accepted or in-flight work.
    pub fn discard(&mut self) {
        self.discard_revision = self
            .discard_revision
            .checked_add(1)
            .expect("discard revision exhausted");
        self.inputs.discard();
        for id in self.dirty.drain() {
            self.banks.get_mut(&id).unwrap().discard();
        }
        self.pending_root = None;
        self.pending_links.clear();
        for handle in self.born.drain() {
            self.timing.remove(handle);
            let pool = self.banks.get_mut(&handle.bank).unwrap();
            self.ranges.remove(&pool.extent(handle).unwrap().start);
            pool.remove(handle).unwrap();
        }
    }

    /// Resolve synchronously on the owner, without admitting new worker jobs.
    pub fn update_inline(&mut self, size: (u32, u32), scale: f32) -> Result<Work> {
        let update = self.advance(size, scale, None)?;
        match update.completion? {
            Poll::Ready(_) => Ok(update.work),
            Poll::Pending => Err("layout job already in flight"),
        }
    }

    pub fn advance(
        &mut self,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
    ) -> Result<Update> {
        let time = self.clock.as_ref().map(|_| FrameTime::now());
        self.advance_inner(size, scale, time, execution)
    }

    pub fn advance_at(
        &mut self,
        size: (u32, u32),
        scale: f32,
        time: FrameTime,
        execution: Option<&LayoutExecution>,
    ) -> Result<Update> {
        self.advance_inner(size, scale, Some(time), execution)
    }

    fn advance_inner(
        &mut self,
        size: (u32, u32),
        scale: f32,
        time: Option<FrameTime>,
        execution: Option<&LayoutExecution>,
    ) -> Result<Update> {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Ir);
        #[cfg(feature = "profiling")]
        {
            self.profile = crate::runtime::UpdateProfile::default();
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        self.scene.stats = crate::scene::UpdateStats {
            visible_chunks: self.scene.stats.visible_chunks,
            ..Default::default()
        };
        let mut topology = self
            .topology_changed()
            .then(|| self.topology())
            .transpose()?;
        let revision = self.topology_revision;
        let evaluated = crate::runtime::profile::timed!(
            self,
            evaluation,
            self.evaluate_inputs(topology.as_ref(), time)
        )?;
        if self.topology_revision != revision {
            topology = self
                .topology_changed()
                .then(|| self.topology())
                .transpose()?;
        }
        if self.admission_idle(size, scale) {
            return Ok(self.result(evaluated, Ok(Poll::Ready(Damage::None))));
        }
        let update = crate::runtime::profile::timed!(
            self,
            scene,
            self.advance_layout(size, scale, execution, topology)
        );
        let input_updates = usize::from(crate::runtime::profile::timed!(
            self,
            input,
            self.sync_input()
        ));
        self.collect_retired();
        let mut update = update?;
        update.work.owners_evaluated = evaluated.owners_evaluated;
        update.work.invalid_deadlines = evaluated.invalid_deadlines;
        update.work.input_updates = input_updates;
        Ok(update)
    }

    fn admission_idle(&self, size: (u32, u32), scale: f32) -> bool {
        self.dirty.is_empty()
            && self.pending_root.is_none()
            && self.pending_links.is_empty()
            && self.born.is_empty()
            && self.retired.is_empty()
            && !self.inputs.has_pending()
            && !self.failed
            && self.scene.pending_layout().is_none()
            && self.scene.frame().size() == size
            && self.scene.frame().scale() == scale
    }

    fn advance_layout(
        &mut self,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
        topology: Option<Topology>,
    ) -> Result<Update> {
        let mut prior_damage = Damage::None;
        if let Some(viewport) = self.scene.pending_layout() {
            let current = viewport == (size, scale) && !self.has_effects();
            match self.scene.poll_layout(current) {
                Ok(Poll::Pending) => return Ok(self.result(Work::default(), Ok(Poll::Pending))),
                Ok(Poll::Ready(Some(damage))) => {
                    self.publish();
                    prior_damage = damage;
                }
                Ok(Poll::Ready(None)) => self.failed = true,
                Err(error) => {
                    self.failed = true;
                    return Ok(self.result(Work::default(), Err(error)));
                }
            }
        }
        let mut changes = std::mem::take(&mut self.changes);
        let result =
            self.apply_pending(size, scale, execution, prior_damage, &mut changes, topology);
        changes.clear();
        if !self.active.is_empty() || !self.visible.is_empty() {
            self.changes = changes;
        }
        result
    }

    fn apply_pending(
        &mut self,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
        prior_damage: Damage,
        changes: &mut Edits,
        topology: Option<Topology>,
    ) -> Result<Update> {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Bindings);
        let active = topology.as_ref().map(|plan| {
            self.active
                .intersection(&plan.active)
                .copied()
                .collect::<Set<_>>()
        });
        let mut work = Work::default();
        for &id in &self.dirty {
            let prepared = self
                .banks
                .get_mut(&id)
                .unwrap()
                .prepare(active.as_ref().unwrap_or(&self.active), changes)?;
            work.sources_compared += prepared.sources_compared;
            work.bindings_emitted += prepared.bindings_emitted;
            work.instances_changed += prepared.instances_changed;
        }
        let mut roots = Vec::new();
        if let Some(plan) = &topology {
            work.topology_visited = plan.active.len();
            #[cfg(feature = "profiling")]
            let started = std::time::Instant::now();
            let mut children = Vec::new();
            if let Some(root) = plan.root {
                self.lower(root, plan, &mut work, &mut children, changes)?;
            }
            roots.push(Subtree::Replace(
                Box::new(Self::host_element(self.host)),
                children,
            ));
            #[cfg(feature = "profiling")]
            {
                self.profile.lowering += started.elapsed();
                self.profile.existing_nodes_materialized = work.existing_nodes_materialized;
            }
        }
        let needs_backend = !roots.is_empty()
            || !changes.scene.is_empty()
            || self.failed
            || self.scene.frame().size() != size
            || self.scene.frame().scale() != scale;
        let future_active = topology.as_ref().map_or(&self.active, |plan| &plan.active);
        if self.host_routes_input {
            changes.inputs.clear();
        }
        let inputs_changed = self.inputs.changed(future_active)
            || changes.inputs.iter().any(|(key, input)| {
                let address = self.locate(*key).unwrap();
                future_active.contains(&address.instance)
                    && self.inputs.derived_changed(address, input)
            });
        let completion = if needs_backend || inputs_changed {
            let generation = self
                .generation
                .checked_add(1)
                .ok_or("graph generation exhausted")?;
            let completion = if needs_backend {
                self.scene
                    .transact(roots, &mut changes.scene, size, scale, execution)?
                    .completion
            } else {
                Ok(Poll::Ready(prior_damage))
            };
            self.generation = generation;
            completion
        } else {
            Ok(Poll::Ready(prior_damage))
        };
        for id in self.dirty.drain() {
            self.banks.get_mut(&id).unwrap().accept();
        }
        if let Some(plan) = topology {
            work.instances_unmounted = self.active.difference(&plan.active).count();
            for handle in self.active.difference(&plan.active) {
                self.timing.remove(*handle);
            }
            self.retired
                .extend(self.active.difference(&plan.active).copied());
            self.active = plan.active;
            self.root = plan.root;
            self.unpublished_parents = plan.parent_changes;
            self.topology_unpublished = true;
        }
        for (key, input) in changes.inputs.drain(..) {
            let address = self.locate(key).unwrap();
            if self.active.contains(&address.instance) {
                self.inputs.accept_derived(address, input);
            }
        }
        self.names.accept(changes.names.drain(..));
        self.inputs.accept(&self.active);
        self.links.extend(self.pending_links.drain());
        self.pending_root = None;
        work.instances_mounted = self.born.intersection(&self.active).count();
        self.retired.extend(
            self.born
                .drain()
                .filter(|handle| !self.active.contains(handle)),
        );
        self.failed = completion.is_err();
        if matches!(completion, Ok(Poll::Ready(_))) {
            self.publish();
        }
        Ok(self.result(work, completion))
    }

    fn result(&self, work: Work, completion: Result<Poll<Damage>>) -> Update {
        Update {
            accepted: self.generation,
            published: self.published,
            work,
            completion,
        }
    }
}
