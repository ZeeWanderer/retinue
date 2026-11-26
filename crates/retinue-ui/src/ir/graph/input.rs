use super::super::edits::Input as Binding;
use super::*;
use crate::{
    input::{Dispatch, Event, HitRegion, InputOptions, Router, Target},
    runtime::ViewId,
    scene::Geometry,
};
use std::ops::Range;

#[cfg(test)]
mod tests;

struct Route {
    address: Address,
    parent: Option<ViewId>,
    depth: usize,
}

#[derive(Clone, Copy, Default)]
struct Ancestry {
    parent: Option<ViewId>,
    depth: usize,
}

#[derive(Default)]
struct Placements {
    sources: Vec<usize>,
    targets: Vec<Range<usize>>,
}

fn regions<'a>(
    geometry: &'a [Geometry],
    sources: &'a [usize],
) -> impl ExactSizeIterator<Item = HitRegion> + 'a {
    sources.iter().map(|&i| HitRegion {
        bounds: geometry[i].layout,
        clip: geometry[i].clip,
    })
}

#[derive(Default)]
pub(super) struct Inputs {
    bindings: Map<Address, Binding>,
    pending: Map<Address, Option<Binding>>,
    router: Router,
    changed: Set<Address>,
    topology_dirty: bool,
    geometry_revision: u64,
    routes: Vec<Route>,
    routes_dirty: bool,
    placements: Placements,
    #[cfg(test)]
    motion_patches: usize,
    #[cfg(test)]
    route_builds: usize,
}

impl Inputs {
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn derived_changed(&self, address: Address, input: &Option<Binding>) -> bool {
        !self.pending.contains_key(&address) && self.bindings.get(&address) != input.as_ref()
    }

    pub fn accept_derived(&mut self, address: Address, input: Option<Binding>) {
        if self.derived_changed(address, &input) {
            if let Some(input) = input {
                self.routes_dirty |= self.bindings.insert(address, input).is_none();
            } else {
                self.routes_dirty |= self.bindings.remove(&address).is_some();
            }
            self.changed.insert(address);
        }
    }

    pub fn changed(&self, active: &Set<Handle>) -> bool {
        self.pending.iter().any(|(address, binding)| {
            active.contains(&address.instance) && self.bindings.get(address) != binding.as_ref()
        })
    }

    pub fn accept(&mut self, active: &Set<Handle>) {
        for (address, input) in self.pending.drain() {
            if !active.contains(&address.instance) || self.bindings.get(&address) == input.as_ref()
            {
                continue;
            }
            if let Some(input) = input {
                self.routes_dirty |= self.bindings.insert(address, input).is_none();
            } else {
                self.routes_dirty |= self.bindings.remove(&address).is_some();
            }
            self.changed.insert(address);
        }
    }

    pub fn discard(&mut self) {
        self.pending.clear();
    }

    pub fn retain(&mut self, active: &Set<Handle>, visible: &Set<Handle>) {
        let previous = self.bindings.len();
        self.bindings.retain(|address, _| {
            active.contains(&address.instance) || visible.contains(&address.instance)
        });
        self.routes_dirty |= self.bindings.len() != previous;
    }

    pub fn topology_changed(&mut self) {
        self.topology_dirty = true;
    }
}

impl InstanceGraph {
    /// Stage an input owner independently of its layout boxes. The compiler can
    /// bind a Group or Slot without inventing a layout node or duplicate focus IDs.
    /// Explicit edits override evaluated bindings for this admission only.
    pub fn bind_input(
        &mut self,
        address: Address,
        enabled: bool,
        input: Option<InputOptions>,
    ) -> Result<()> {
        if self.host_routes_input {
            return Err("input is routed by the host");
        }
        self.key(address.instance, address.local)?;
        self.inputs
            .pending
            .insert(address, input.map(|options| Binding { enabled, options }));
        Ok(())
    }

    pub fn input(&self) -> &Router {
        &self.inputs.router
    }

    pub fn dispatch(&mut self, event: &Event) -> Dispatch {
        self.inputs.router.dispatch(event)
    }

    pub fn input_address(&self, id: ViewId) -> Option<Address> {
        self.inputs.router.target(id)?;
        self.address(Key::new("ui.ir.node", id.0))
    }

    pub fn input_key(&self, id: ViewId) -> Option<Key> {
        let address = self.input_address(id)?;
        self.enclosing_name(self.key(address.instance, address.local).ok()?)
    }

    pub fn hit_test(&self, position: (f32, f32)) -> Option<Key> {
        self.input_key(self.inputs.router.hit_test(position)?)
    }

    pub fn focus(&mut self, address: Option<Address>) -> Result<bool> {
        let id = address
            .map(|address| {
                self.key(address.instance, address.local)
                    .map(|key| ViewId(key.value))
            })
            .transpose()?;
        Ok(self.inputs.router.focus(id))
    }

    fn input_sources(&self, address: Address, sources: &mut Vec<usize>, order: &mut usize) {
        let pool = self.pool(address.instance).unwrap();
        let structure = pool.structure(address.instance).unwrap();
        match structure.nodes[address.local.0 as usize].kind {
            Kind::Group => {
                for &local in structure.layout_children(address.local) {
                    self.input_sources(Address { local, ..address }, sources, order);
                }
            }
            Kind::Slot => {
                let port = Port {
                    owner: address.instance,
                    local: address.local,
                };
                for child in self.links.get(&port).into_iter().flatten() {
                    self.input_sources(
                        Address {
                            instance: child.handle,
                            local: Local(0),
                        },
                        sources,
                        order,
                    );
                }
            }
            _ => {
                let key = pool.key(address.instance, address.local).unwrap();
                if let Some((index, enabled)) = self.scene.input_source(key) {
                    *order = (*order).min(index);
                    if enabled {
                        sources.push(index);
                    }
                }
            }
        }
    }

    fn rebuild_input_routes(&mut self) {
        let mut routes = std::mem::take(&mut self.inputs.routes);
        routes.clear();
        routes.reserve(self.inputs.bindings.len());
        let mut prefixes = Map::default();
        let mut path = Vec::new();
        for &address in self.inputs.bindings.keys() {
            if !self.is_visible(address.instance) {
                continue;
            }
            let mut prefix = Ancestry::default();
            let mut ancestor = self.parent(address);
            while let Some(at) = ancestor {
                if let Some(&known) = prefixes.get(&at) {
                    prefix = known;
                    break;
                }
                path.push(at);
                ancestor = self.parent(at);
            }
            for at in path.drain(..).rev() {
                prefix.depth += 1;
                if self.inputs.bindings.contains_key(&at) {
                    prefix.parent = Some(ViewId(self.key(at.instance, at.local).unwrap().value));
                }
                prefixes.insert(at, prefix);
            }
            routes.push(Route {
                address,
                parent: prefix.parent,
                depth: prefix.depth,
            });
        }
        self.inputs.routes = routes;
        self.inputs.routes_dirty = false;
        #[cfg(test)]
        {
            self.inputs.route_builds += 1;
        }
    }

    pub(super) fn sync_input(&mut self) -> bool {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Input);
        if self.published != self.generation {
            return false;
        }
        let revision = self.scene.input_revision();
        let same_geometry =
            !self.inputs.topology_dirty && self.inputs.geometry_revision == revision;
        if self.inputs.changed.is_empty() && same_geometry {
            return false;
        }
        if same_geometry
            && self.inputs.changed.iter().all(|address| {
                self.inputs.bindings.get(address).is_some_and(|binding| {
                    let id = ViewId(self.key(address.instance, address.local).unwrap().value);
                    self.inputs
                        .router
                        .can_patch_handler(id, binding.enabled, &binding.options)
                })
            })
        {
            for address in &self.inputs.changed {
                let id = ViewId(self.key(address.instance, address.local).unwrap().value);
                let binding = &self.inputs.bindings[address];
                self.inputs
                    .router
                    .patch_handler(id, binding.options.handler.clone());
            }
            self.inputs.changed.clear();
            return true;
        }
        if self.inputs.bindings.is_empty() && self.inputs.router.targets().is_empty() {
            self.inputs.routes.clear();
            self.inputs.placements = Placements::default();
            self.inputs.routes_dirty = false;
            self.inputs.changed.clear();
            self.inputs.topology_dirty = false;
            self.inputs.geometry_revision = revision;
            return false;
        }
        if !self.inputs.topology_dirty
            && !self.inputs.routes_dirty
            && self.inputs.changed.is_empty()
            && let Some(moved) = self.scene.input_motion_since(self.inputs.geometry_revision)
        {
            let geometry = self.scene.geometry();
            let plan = &self.inputs.placements;
            let changed =
                self.inputs
                    .router
                    .place_targets(plan.targets.iter().enumerate().filter_map(
                        |(target, range)| {
                            let sources = &plan.sources[range.clone()];
                            moved
                                .touches(sources)
                                .then(|| (target, regions(geometry, sources)))
                        },
                    ));
            self.inputs.geometry_revision = revision;
            #[cfg(test)]
            {
                self.inputs.motion_patches += 1;
            }
            return changed;
        }
        if self.inputs.routes_dirty || self.inputs.topology_dirty {
            self.rebuild_input_routes();
        }
        let mut targets = Vec::with_capacity(self.inputs.routes.len());
        let mut plan = std::mem::take(&mut self.inputs.placements);
        plan.sources.clear();
        plan.targets.clear();
        for route in &self.inputs.routes {
            let address = route.address;
            let binding = &self.inputs.bindings[&address];
            let start = plan.sources.len();
            let mut order = usize::MAX;
            self.input_sources(address, &mut plan.sources, &mut order);
            let range = start..plan.sources.len();
            let mut target = Target {
                id: ViewId(self.key(address.instance, address.local).unwrap().value),
                parent: route.parent,
                bounds: Default::default(),
                clip: Default::default(),
                regions: None,
                enabled: binding.enabled && !range.is_empty(),
                input: binding.options.clone(),
            };
            target.place(regions(self.scene.geometry(), &plan.sources[range.clone()]));
            targets.push((order, route.depth, target, range));
        }
        targets.sort_unstable_by_key(|(order, depth, target, _)| (*order, *depth, target.id));
        self.inputs.router.update(
            targets
                .into_iter()
                .map(|(_, _, target, range)| {
                    plan.targets.push(range);
                    target
                })
                .collect(),
        );
        self.inputs.placements = plan;
        self.inputs.geometry_revision = revision;
        self.inputs.changed.clear();
        self.inputs.topology_dirty = false;
        true
    }
}
