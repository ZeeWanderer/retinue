use super::{
    HostPolicy, Key, ProgramId, RenderHost, ScopeId, execution::Materialization,
    materialization::Recipe, program::RasterSlot,
};
use crate::{
    Rect,
    painter::{ImageStorage, RasterImage},
    resources::ResourceId,
};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub struct ResidencyPolicy {
    pub warm_bytes: usize,
    pub warm_lifetime: Duration,
    pub max_surface_bytes: usize,
    pub peak_bytes: usize,
}

impl ResidencyPolicy {
    pub const fn desktop(warm_bytes: usize) -> Self {
        Self {
            warm_bytes,
            warm_lifetime: Duration::from_secs(2),
            max_surface_bytes: 12 * 1024 * 1024,
            peak_bytes: 128 * 1024 * 1024,
        }
    }

    pub const fn overlay(warm_bytes: usize) -> Self {
        Self {
            warm_bytes,
            warm_lifetime: Duration::from_millis(500),
            max_surface_bytes: 512 * 1024,
            peak_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ResidencyStats {
    pub entries: usize,
    pub selected_bytes: usize,
    pub warm_bytes: usize,
    pub warm_limit_bytes: usize,
    pub retired_bytes: usize,
    pub live_bytes: usize,
    pub pending_bytes: usize,
    pub peak_limit_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct RasterId(ResourceId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Owner {
    Item(ProgramId),
    Group(ScopeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Retention {
    Discard,
    Nearby,
}

pub(super) struct Reservation {
    pending: Rc<Cell<usize>>,
    bytes: usize,
    _host: super::host::Reservation,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.pending.set(self.pending.get() - self.bytes);
    }
}

struct Identity {
    size: (u32, u32),
    fragments: Vec<(Key, ProgramId, (i32, i32), Rect)>,
}

impl Identity {
    fn new(recipe: &Recipe) -> Self {
        Self {
            size: recipe.size,
            fragments: recipe
                .fragments
                .iter()
                .map(|f| (f.key, f.program.id(), f.origin, f.clip))
                .collect(),
        }
    }

    fn matches(&self, recipe: &Recipe, owner: Owner) -> bool {
        self.size == recipe.size
            && self.fragments.len() == recipe.fragments.len()
            && self.fragments.iter().zip(recipe.fragments.iter()).all(
                |(&(key, program, origin, clip), f)| {
                    (matches!(owner, Owner::Item(_)) || key == f.key)
                        && program == f.program.id()
                        && origin == f.origin
                        && clip == f.clip
                },
            )
    }

    fn bytes(&self) -> usize {
        self.fragments.capacity() * size_of::<(Key, ProgramId, (i32, i32), Rect)>()
    }
}

struct Entry {
    owner: Owner,
    slot: Option<RasterSlot>,
    allocation: ResourceId,
    image: Option<Rc<RasterImage>>,
    recipe: Option<Recipe>,
    identity: Identity,
    consumers: usize,
    nearby: bool,
    draws: u64,
    preparation_ns: u64,
    warm_cost: usize,
}

struct Allocation {
    storage: ImageStorage,
    bytes: usize,
    selected: usize,
    warm: usize,
}

pub(super) struct Residency {
    pub policy: ResidencyPolicy,
    pub host: RenderHost,
    entries: HashMap<RasterId, Entry>,
    owners: HashMap<Owner, Vec<RasterId>>,
    items: HashMap<RasterSlot, Vec<RasterId>>,
    allocations: HashMap<ResourceId, Allocation>,
    retired: HashSet<ResourceId>,
    warm: HashMap<RasterId, Instant>,
    warm_bytes: usize,
    tracked_bytes: usize,
    reclaimed: u64,
    pending: Rc<Cell<usize>>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl Residency {
    pub fn new(policy: ResidencyPolicy) -> Self {
        Self::with_host(
            policy,
            RenderHost::new(HostPolicy::standalone(policy.peak_bytes)),
        )
    }

    pub fn with_host(policy: ResidencyPolicy, host: RenderHost) -> Self {
        Self {
            policy,
            host,
            entries: HashMap::new(),
            owners: HashMap::new(),
            items: HashMap::new(),
            allocations: HashMap::new(),
            retired: HashSet::new(),
            warm: HashMap::new(),
            warm_bytes: 0,
            tracked_bytes: 0,
            reclaimed: 0,
            pending: Rc::default(),
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub fn matches(&self, id: RasterId, recipe: &Recipe) -> bool {
        let entry = &self.entries[&id];
        entry.identity.matches(recipe, entry.owner)
    }

    pub fn is_item(&self, id: RasterId, program: ProgramId) -> bool {
        self.entries[&id].owner == Owner::Item(program)
    }

    pub fn acquire(&mut self, owner: Owner, recipe: &Recipe) -> Option<RasterId> {
        let id = self
            .owners
            .get(&owner)
            .into_iter()
            .flatten()
            .rev()
            .copied()
            .find(|id| {
                (self.entries[id].consumers != 0 || self.host.has_warm(*id))
                    && self.matches(*id, recipe)
            });
        if let Some(id) = id {
            self.retain(id, recipe);
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        id
    }

    pub fn retain(&mut self, id: RasterId, recipe: &Recipe) {
        let entry = self.entries.get_mut(&id).expect("live raster");
        entry.nearby = false;
        if entry.consumers == 0 {
            self.warm.remove(&id);
            self.warm_bytes -= entry.warm_cost;
            entry.image = Some(self.host.take_warm(id).expect("live warm retention"));
            let allocation = self.allocations.get_mut(&entry.allocation).unwrap();
            allocation.warm -= 1;
            allocation.selected += 1;
            self.host.transition(entry.allocation, 1, 0);
            entry.recipe = Some(recipe.clone());
        }
        entry.consumers += 1;
    }

    pub fn insert(
        &mut self,
        owner: Owner,
        value: Materialization,
        preparation_ns: u64,
    ) -> RasterId {
        let id = RasterId(ResourceId::next());
        let allocation = self.allocations.entry(value.image.id()).or_insert_with(|| {
            let bytes = value.image.byte_len();
            self.tracked_bytes += bytes;
            Allocation {
                storage: value.image.storage(),
                bytes,
                selected: 0,
                warm: 0,
            }
        });
        allocation.selected += 1;
        self.host.track(&value.image);
        self.retired.remove(&value.image.id());
        let identity = Identity::new(&value.recipe);
        let warm_cost = value.image.byte_len() + identity.bytes() + size_of::<Entry>();
        let slot = matches!(owner, Owner::Item(_))
            .then(|| value.recipe.fragments[0].program.signature().slot);
        if let Some(slot) = slot {
            self.items.entry(slot).or_default().push(id);
        }
        self.entries.insert(
            id,
            Entry {
                owner,
                slot,
                allocation: value.image.id(),
                image: Some(value.image),
                recipe: Some(value.recipe),
                identity,
                consumers: 1,
                nearby: false,
                draws: 0,
                preparation_ns,
                warm_cost,
            },
        );
        self.owners.entry(owner).or_default().push(id);
        id
    }

    pub fn image(&self, id: RasterId) -> &Rc<RasterImage> {
        self.entries[&id].image.as_ref().expect("selected raster")
    }

    pub fn materialization(&self, id: RasterId) -> Materialization {
        let entry = &self.entries[&id];
        Materialization {
            image: entry.image.as_ref().expect("selected raster").clone(),
            recipe: entry.recipe.as_ref().expect("selected raster").clone(),
        }
    }

    pub fn previous_item(&self, slot: RasterSlot) -> Option<Materialization> {
        self.items
            .get(&slot)
            .into_iter()
            .flatten()
            .rev()
            .find_map(|id| {
                let entry = &self.entries[id];
                Some(Materialization {
                    image: entry.image.clone()?,
                    recipe: entry.recipe.as_ref()?.clone(),
                })
            })
    }

    pub fn drawn(&mut self, id: RasterId) {
        let entry = self.entries.get_mut(&id).unwrap();
        entry.draws = entry.draws.saturating_add(1);
    }

    pub fn release(&mut self, id: RasterId, retention: Retention, now: Instant) {
        let entry = self
            .entries
            .get_mut(&id)
            .expect("balanced raster consumers");
        assert!(entry.consumers > 0);
        entry.nearby |= retention == Retention::Nearby;
        entry.consumers -= 1;
        if entry.consumers != 0 {
            return;
        }
        self.allocations
            .get_mut(&entry.allocation)
            .unwrap()
            .selected -= 1;
        self.host.transition(entry.allocation, -1, 0);
        let deadline = now.checked_add(self.policy.warm_lifetime);
        let keep = entry.nearby
            && entry.draws > 0
            && !self.policy.warm_lifetime.is_zero()
            && deadline.is_some()
            && entry.warm_cost <= self.policy.warm_bytes;
        if keep && self.warm_room(id) {
            let entry = self.entries.get_mut(&id).unwrap();
            // Warm exact reuse needs identities, not source programs and their asset leases.
            entry.recipe = None;
            self.allocations.get_mut(&entry.allocation).unwrap().warm += 1;
            let deadline = deadline.unwrap();
            self.host
                .retain_warm(id, entry.image.take().unwrap(), deadline);
            self.warm_bytes += entry.warm_cost;
            self.warm.insert(id, deadline);
        } else {
            self.remove(id);
        }
    }

    fn warm_room(&mut self, id: RasterId) -> bool {
        let entry = &self.entries[&id];
        let mut needed = entry
            .warm_cost
            .saturating_sub(self.policy.warm_bytes.saturating_sub(self.warm_bytes));
        let mut cheaper: Vec<_> = self
            .warm
            .keys()
            .copied()
            .filter(|candidate| {
                let other = &self.entries[candidate];
                u128::from(other.preparation_ns) * (entry.warm_cost as u128)
                    < u128::from(entry.preparation_ns) * other.warm_cost as u128
            })
            .collect();
        cheaper.sort_unstable_by(|a, b| {
            let a = &self.entries[a];
            let b = &self.entries[b];
            (u128::from(a.preparation_ns) * b.warm_cost as u128)
                .cmp(&(u128::from(b.preparation_ns) * a.warm_cost as u128))
        });
        let mut evict = Vec::new();
        for candidate in cheaper {
            if needed == 0 {
                break;
            }
            needed = needed.saturating_sub(self.entries[&candidate].warm_cost);
            evict.push(candidate);
        }
        if needed != 0 {
            return false;
        }
        for candidate in evict {
            self.remove(candidate);
        }
        true
    }

    fn remove(&mut self, id: RasterId) {
        let entry = self.entries.remove(&id).unwrap();
        assert_eq!(entry.consumers, 0);
        let allocation = self.allocations.get_mut(&entry.allocation).unwrap();
        if self.warm.remove(&id).is_some() {
            self.warm_bytes -= entry.warm_cost;
            allocation.warm -= 1;
            self.host.take_warm(id);
            self.evictions += 1;
        }
        if allocation.selected == 0 && allocation.warm == 0 {
            self.retired.insert(entry.allocation);
        }
        let versions = self.owners.get_mut(&entry.owner).unwrap();
        versions.retain(|v| *v != id);
        if versions.is_empty() {
            self.owners.remove(&entry.owner);
        }
        if let Some(slot) = entry.slot {
            let versions = self.items.get_mut(&slot).unwrap();
            versions.retain(|v| *v != id);
            if versions.is_empty() {
                self.items.remove(&slot);
            }
        }
    }

    fn reap(&mut self) {
        self.retired.retain(|id| {
            if self.allocations[id].storage.is_alive() {
                return true;
            }
            self.tracked_bytes -= self.allocations.remove(id).unwrap().bytes;
            self.reclaimed = self.reclaimed.wrapping_add(1);
            false
        });
    }

    pub fn reclaimed(&self) -> u64 {
        self.reclaimed.wrapping_add(self.host.reclaimed())
    }

    pub fn eligible(&self, recipe: &Recipe) -> bool {
        if recipe.size.0 > 8192 || recipe.size.1 > 8192 {
            return false;
        }
        let bytes = recipe.size.0 as u64 * recipe.size.1 as u64 * 4;
        bytes > 0
            && bytes <= self.policy.max_surface_bytes as u64
            && bytes <= self.policy.peak_bytes as u64
    }

    pub fn maintain(&mut self, now: Instant) {
        self.host.maintain(now);
        let expired: Vec<_> = self
            .warm
            .iter()
            .filter_map(|(&id, &until)| (until <= now || !self.host.has_warm(id)).then_some(id))
            .collect();
        for id in expired {
            self.remove(id);
        }
        self.reap();
    }

    pub fn trim_warm(&mut self) {
        let ids: Vec<_> = self.warm.keys().copied().collect();
        for id in ids {
            self.remove(id);
        }
        self.reap();
    }

    pub fn retain_warm(&mut self, valid: impl Fn(Owner) -> bool) {
        let obsolete: Vec<_> = self
            .warm
            .keys()
            .copied()
            .filter(|id| !valid(self.entries[id].owner))
            .collect();
        for id in obsolete {
            self.remove(id);
        }
        self.reap();
    }

    pub fn reserve(&mut self, recipe: &Recipe) -> Option<Reservation> {
        if !self.eligible(recipe) {
            return None;
        }
        let bytes = (recipe.size.0 as usize)
            .checked_mul(recipe.size.1 as usize)?
            .checked_mul(4)?;
        if bytes == 0 || bytes > self.policy.max_surface_bytes {
            return None;
        }
        if self
            .tracked_bytes
            .saturating_add(self.pending.get())
            .saturating_add(bytes)
            > self.policy.peak_bytes
        {
            self.trim_warm();
        }
        if self
            .tracked_bytes
            .saturating_add(self.pending.get())
            .saturating_add(bytes)
            > self.policy.peak_bytes
        {
            return None;
        }
        let reservation = self.host.reserve(bytes).or_else(|| {
            self.trim_warm();
            self.host.reserve(bytes)
        })?;
        self.pending.set(self.pending.get() + bytes);
        Some(Reservation {
            pending: self.pending.clone(),
            bytes,
            _host: reservation,
        })
    }

    pub fn clear(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.consumers > 0 {
                self.host.transition(entry.allocation, -1, 0);
            }
            entry.consumers = 0;
        }
        for allocation in self.allocations.values_mut() {
            allocation.selected = 0;
        }
        let ids: Vec<_> = self.entries.keys().copied().collect();
        for id in ids {
            self.remove(id);
        }
        self.reap();
    }

    pub fn stats(&self) -> ResidencyStats {
        let mut result = ResidencyStats {
            entries: self.entries.len(),
            warm_limit_bytes: self.policy.warm_bytes,
            pending_bytes: self.pending.get(),
            peak_limit_bytes: self.policy.peak_bytes,
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            ..Default::default()
        };
        let retained: HashSet<_> = self
            .warm
            .keys()
            .filter(|id| self.host.has_warm(**id))
            .map(|id| self.entries[id].allocation)
            .collect();
        for (id, allocation) in self
            .allocations
            .iter()
            .filter(|(_, a)| a.storage.is_alive())
        {
            result.live_bytes += allocation.bytes;
            if allocation.selected != 0 {
                result.selected_bytes += allocation.bytes;
            } else if retained.contains(id) {
                result.warm_bytes += allocation.bytes;
            } else {
                result.retired_bytes += allocation.bytes;
            }
        }
        result
    }
}

impl Drop for Residency {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests;
