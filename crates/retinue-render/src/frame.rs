use super::{
    CacheHint, CachePolicy, ClipId, CollectionContext, Key, OptimizationContext, PaintProgram,
    PropertyBinding, PropertyChange, PropertyDefinitions, PropertyRevision, PropertyStats,
    TransformId, properties::Properties,
};
use crate::{
    Rect,
    damage::{Damage, Region},
};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::{borrow::Cow, collections::VecDeque, ops::Range, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintPlacement {
    Pixels { origin: (i32, i32), clip: Rect },
    Properties(PropertyBinding),
}

/// Placement can be resolved pixels or references to shared logical properties.
#[derive(Clone)]
pub struct PaintInstance {
    pub key: Key,
    pub program: PaintProgram,
    pub(crate) placement: PaintPlacement,
    pub(crate) origin: (i32, i32),
    pub(crate) clip: Rect,
    pub cache: CachePolicy,
    pub(crate) visible: Rect,
}

impl PaintInstance {
    pub fn new(key: Key, program: PaintProgram, origin: (i32, i32), clip: Rect) -> Self {
        Self {
            key,
            visible: program.bounds().visible(origin, clip),
            program,
            origin,
            clip,
            placement: PaintPlacement::Pixels { origin, clip },
            cache: CachePolicy::Never,
        }
    }

    pub fn visible_bounds(&self) -> Rect {
        self.program.bounds().visible(self.origin, self.clip)
    }

    pub fn with_properties(key: Key, program: PaintProgram, properties: PropertyBinding) -> Self {
        let mut instance = Self::new(key, program, (0, 0), Rect::default());
        instance.placement = PaintPlacement::Properties(properties);
        instance
    }

    pub fn origin(&self) -> (i32, i32) {
        self.origin
    }
    pub fn clip(&self) -> Rect {
        self.clip
    }
    pub fn placement(&self) -> PaintPlacement {
        self.placement
    }

    pub fn set_pixels(&mut self, origin: (i32, i32), clip: Rect) {
        self.placement = PaintPlacement::Pixels { origin, clip };
        self.origin = origin;
        self.clip = clip;
        self.visible = self.visible_bounds();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaintGroup {
    pub key: Key,
    pub instances: Range<usize>,
    pub context: OptimizationContext,
}

impl PaintGroup {
    pub fn cache_hint(&self) -> CacheHint {
        self.context.get::<CacheHint>().copied().unwrap_or_default()
    }
    pub fn collection(&self) -> Option<&CollectionContext> {
        self.context.get()
    }
}

pub struct PaintFrame {
    pub(crate) identity: Rc<()>,
    pub(crate) generation: u64,
    pub(crate) size: (u32, u32),
    pub(crate) scale: f32,
    pub(crate) chunks: Vec<PaintInstance>,
    pub(crate) groups: Vec<PaintGroup>,
    pub(crate) changes: Changes,
    history: VecDeque<Changes>,
    pub(crate) properties: Properties,
}

pub struct PaintUpdate {
    pub index: usize,
    pub program: PaintProgram,
    pub cache: CachePolicy,
}

pub struct PlacementUpdate {
    pub index: usize,
    pub origin: (i32, i32),
    pub clip: Rect,
}

pub struct GroupUpdate {
    pub index: usize,
    pub context: OptimizationContext,
}

#[derive(Clone, Default)]
pub(crate) struct Changes {
    pub from: u64,
    pub topology: bool,
    pub instances: Vec<usize>,
    pub groups: Vec<usize>,
}

impl Default for PaintFrame {
    fn default() -> Self {
        Self {
            identity: Rc::new(()),
            generation: 0,
            size: (0, 0),
            scale: 1.0,
            chunks: Vec::new(),
            groups: Vec::new(),
            changes: Changes::default(),
            history: VecDeque::new(),
            properties: Properties::default(),
        }
    }
}

impl PaintFrame {
    pub(crate) fn begin_change(&mut self, topology: bool) {
        const HISTORY: usize = 4;
        if topology || self.changes.topology {
            self.history.clear();
        } else if self.generation > 0 {
            let recycled = if self.history.len() == HISTORY {
                self.history.pop_front().unwrap()
            } else {
                Changes::default()
            };
            self.history
                .push_back(std::mem::replace(&mut self.changes, recycled));
        }
        self.changes.from = self.generation;
        self.changes.topology = topology;
        self.changes.instances.clear();
        self.changes.groups.clear();
        self.generation += 1;
    }

    pub(crate) fn changes_since(&self, generation: u64) -> Option<Cow<'_, Changes>> {
        if generation == self.changes.from {
            return Some(Cow::Borrowed(&self.changes));
        }
        let first = self
            .history
            .iter()
            .position(|change| change.from == generation)?;
        let mut merged = Changes {
            from: generation,
            ..Changes::default()
        };
        for change in self.history.iter().skip(first).chain([&self.changes]) {
            merged.topology |= change.topology;
            merged.instances.extend(&change.instances);
            merged.groups.extend(&change.groups);
        }
        merged.instances.sort_unstable();
        merged.instances.dedup();
        merged.groups.sort_unstable();
        merged.groups.dedup();
        Some(Cow::Owned(merged))
    }

    pub(crate) fn changed(&mut self, index: usize) {
        self.changes.instances.push(index);
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn instances(&self) -> &[PaintInstance] {
        &self.chunks
    }

    pub fn groups(&self) -> &[PaintGroup] {
        &self.groups
    }

    pub fn has_external_paint(&self) -> bool {
        self.chunks
            .iter()
            .any(|instance| !instance.visible.is_empty() && instance.program.summary().external)
    }

    pub fn replace(
        &mut self,
        size: (u32, u32),
        scale: f32,
        instances: Vec<PaintInstance>,
        groups: Vec<PaintGroup>,
    ) -> Result<Damage, &'static str> {
        self.replace_with_properties(
            size,
            scale,
            instances,
            groups,
            PropertyDefinitions::default(),
        )
    }

    pub fn replace_with_properties(
        &mut self,
        size: (u32, u32),
        scale: f32,
        mut instances: Vec<PaintInstance>,
        mut groups: Vec<PaintGroup>,
        definitions: PropertyDefinitions,
    ) -> Result<Damage, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("paint scale must be positive and finite");
        }
        let mut keys = HashSet::default();
        for instance in &instances {
            if !keys.insert(instance.key) {
                return Err("duplicate paint instance");
            }
            if instance.program.scale() != scale {
                return Err("paint program scale differs from frame");
            }
        }
        keys.clear();
        for group in &groups {
            if !keys.insert(group.key) {
                return Err("duplicate paint group");
            }
            if group.instances.start > group.instances.end || group.instances.end > instances.len()
            {
                return Err("paint group is outside instance range");
            }
            if let Some(context) = group.collection() {
                context.validate(&group.instances)?;
            }
        }
        groups.sort_by_key(|group| {
            (
                group.instances.start,
                std::cmp::Reverse(group.instances.end),
            )
        });
        let mut ends = Vec::new();
        for group in &groups {
            if group.instances.is_empty() {
                continue;
            }
            while ends.last().is_some_and(|end| *end <= group.instances.start) {
                ends.pop();
            }
            if ends.last().is_some_and(|end| *end < group.instances.end) {
                return Err("paint groups must be nested or disjoint");
            }
            ends.push(group.instances.end);
        }
        let mut properties = Properties::build(definitions, size, scale)?;
        let properties_changed = !self.properties.same_values(&properties);
        if !properties_changed {
            properties = self.properties.clone();
        }
        properties.clear_users();
        let viewport = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        for (index, instance) in instances.iter_mut().enumerate() {
            match instance.placement {
                PaintPlacement::Pixels { origin, clip } => {
                    instance.origin = origin;
                    instance.clip = clip.intersection(viewport);
                }
                PaintPlacement::Properties(binding) => {
                    properties.bind(binding, index)?;
                    (instance.origin, instance.clip) = properties.resolve(binding);
                }
            }
            instance.visible = instance
                .program
                .bounds()
                .visible(instance.origin, instance.clip);
        }
        let resized = self.size != size || self.scale != scale;
        let (changed, damage) = difference(&self.chunks, &instances);
        if resized || changed || self.groups != groups || properties_changed {
            let topology = resized
                || self.groups.len() != groups.len()
                || self
                    .groups
                    .iter()
                    .zip(&groups)
                    .any(|(a, b)| a.key != b.key || a.instances != b.instances)
                || self.chunks.len() != instances.len()
                || self
                    .chunks
                    .iter()
                    .zip(&instances)
                    .any(|(a, b)| a.key != b.key);
            self.begin_change(topology);
            if !topology {
                self.changes.groups.extend(
                    self.groups
                        .iter()
                        .zip(&groups)
                        .enumerate()
                        .filter_map(|(index, (a, b))| (a != b).then_some(index)),
                );
                self.changes.instances.extend(
                    self.chunks
                        .iter()
                        .zip(&instances)
                        .enumerate()
                        .filter_map(|(index, (a, b))| {
                            (a.origin != b.origin
                                || a.clip != b.clip
                                || a.placement != b.placement
                                || a.cache != b.cache
                                || a.program.signature() != b.program.signature())
                            .then_some(index)
                        }),
                );
            }
            self.size = size;
            self.scale = scale;
            self.chunks = instances;
            self.groups = groups;
            self.properties = properties;
        }
        Ok(if resized { Damage::Full } else { damage })
    }

    pub fn property_stats(&self) -> PropertyStats {
        self.properties.stats
    }

    pub fn annotate_collection(
        &mut self,
        key: Key,
        context: Option<CollectionContext>,
    ) -> Result<(), &'static str> {
        let group = self
            .groups
            .iter()
            .find(|g| g.key == key)
            .ok_or("unknown paint group")?;
        let context = match context {
            Some(context) => group.context.clone().with(context),
            None => group.context.clone().without::<CollectionContext>(),
        };
        self.annotate_group(key, context)
    }

    pub fn annotate_group(
        &mut self,
        key: Key,
        context: OptimizationContext,
    ) -> Result<(), &'static str> {
        let index = self
            .groups
            .iter()
            .position(|g| g.key == key)
            .ok_or("unknown paint group")?;
        if let Some(context) = context.get::<CollectionContext>() {
            context.validate(&self.groups[index].instances)?;
        }
        if self.groups[index].context != context {
            self.begin_change(false);
            self.changes.groups.push(index);
            self.groups[index].context = context;
        }
        Ok(())
    }

    pub fn translation_revision(&self, id: TransformId) -> Option<PropertyRevision> {
        self.properties.translation_revision(id)
    }

    pub fn clip_revision(&self, id: ClipId) -> Option<PropertyRevision> {
        self.properties.clip_revision(id)
    }

    pub fn patch_properties(
        &mut self,
        changes: impl IntoIterator<Item = PropertyChange>,
    ) -> Result<Damage, &'static str> {
        self.patch([], changes)
    }

    /// Content and placement changes share one damage calculation and change journal.
    pub fn patch(
        &mut self,
        updates: impl IntoIterator<Item = PaintUpdate>,
        changes: impl IntoIterator<Item = PropertyChange>,
    ) -> Result<Damage, &'static str> {
        self.patch_changes(updates, [], changes, [])
    }

    pub fn patch_resolved(
        &mut self,
        updates: impl IntoIterator<Item = PaintUpdate>,
        placements: impl IntoIterator<Item = PlacementUpdate>,
        groups: impl IntoIterator<Item = GroupUpdate>,
    ) -> Result<Damage, &'static str> {
        self.patch_changes(updates, placements, [], groups)
    }

    fn patch_changes(
        &mut self,
        updates: impl IntoIterator<Item = PaintUpdate>,
        placements: impl IntoIterator<Item = PlacementUpdate>,
        changes: impl IntoIterator<Item = PropertyChange>,
        groups: impl IntoIterator<Item = GroupUpdate>,
    ) -> Result<Damage, &'static str> {
        let mut updates: Vec<_> = updates.into_iter().collect();
        updates.sort_unstable_by_key(|update| update.index);
        for (position, update) in updates.iter().enumerate() {
            if update.index >= self.chunks.len() {
                return Err("unknown paint instance");
            }
            if position > 0 && updates[position - 1].index == update.index {
                return Err("duplicate paint update");
            }
            if update.program.scale() != self.scale {
                return Err("paint program scale differs from frame");
            }
        }
        updates.retain(|update| {
            let instance = &self.chunks[update.index];
            instance.program.signature() != update.program.signature()
                || instance.cache != update.cache
        });
        let mut placements: Vec<_> = placements.into_iter().collect();
        placements.sort_unstable_by_key(|update| update.index);
        let viewport = Rect {
            width: self.size.0,
            height: self.size.1,
            ..Rect::default()
        };
        for position in 0..placements.len() {
            let update = &placements[position];
            let instance = self
                .chunks
                .get(update.index)
                .ok_or("unknown paint instance")?;
            if position > 0 && placements[position - 1].index == update.index {
                return Err("duplicate placement update");
            }
            if !matches!(instance.placement, PaintPlacement::Pixels { .. }) {
                return Err("resolved placement requires a pixel instance");
            }
            placements[position].clip = update.clip.intersection(viewport);
        }
        placements.retain(|update| {
            let instance = &self.chunks[update.index];
            instance.origin != update.origin || instance.clip != update.clip
        });
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_unstable_by_key(|update| update.index);
        for (position, update) in groups.iter().enumerate() {
            let group = self.groups.get(update.index).ok_or("unknown paint group")?;
            if position > 0 && groups[position - 1].index == update.index {
                return Err("duplicate group update");
            }
            if let Some(collection) = update.context.get::<CollectionContext>() {
                collection.validate(&group.instances)?;
            }
        }
        groups.retain(|update| self.groups[update.index].context != update.context);
        let properties = self.properties.patch(changes)?;
        if updates.is_empty() && placements.is_empty() && groups.is_empty() && !properties.changed {
            return Ok(Damage::None);
        }
        self.begin_change(false);
        for update in groups {
            self.groups[update.index].context = update.context;
            self.changes.groups.push(update.index);
        }
        let mut indices = properties.instances;
        if !updates.is_empty() || !placements.is_empty() {
            indices.extend(updates.iter().map(|update| update.index));
            indices.extend(placements.iter().map(|update| update.index));
            indices.sort_unstable();
            indices.dedup();
        }
        let mut updates = updates.into_iter().peekable();
        let mut placements = placements.into_iter().peekable();
        let mut damage = Vec::new();
        for index in indices {
            let instance = &mut self.chunks[index];
            let before = instance.visible;
            let mut changed = false;
            let mut repaint = false;
            if updates.peek().is_some_and(|update| update.index == index) {
                let update = updates.next().unwrap();
                repaint = instance.program.signature() != update.program.signature();
                instance.program = update.program;
                instance.cache = update.cache;
                changed = true;
            }
            if let PaintPlacement::Properties(binding) = instance.placement {
                let (origin, clip) = self.properties.resolve(binding);
                repaint |= instance.origin != origin || instance.clip != clip;
                instance.origin = origin;
                instance.clip = clip;
            }
            if placements
                .peek()
                .is_some_and(|update| update.index == index)
            {
                let update = placements.next().unwrap();
                instance.placement = PaintPlacement::Pixels {
                    origin: update.origin,
                    clip: update.clip,
                };
                instance.origin = update.origin;
                instance.clip = update.clip;
                repaint = true;
            }
            if repaint {
                instance.visible = instance.visible_bounds();
                damage.extend([before, instance.visible]);
            }
            if changed || repaint {
                self.changed(index);
            }
        }
        Ok(Damage::region(Region::new(damage)))
    }

    /// Retains commands and resources, not external callback state. Snapshots
    /// remain renderer-thread owned, like their font and image resources.
    pub fn snapshot(&self) -> FrameSnapshot {
        FrameSnapshot(Self {
            identity: self.identity.clone(),
            generation: self.generation,
            size: self.size,
            scale: self.scale,
            chunks: self.chunks.clone(),
            groups: self.groups.clone(),
            changes: self.changes.clone(),
            history: self.history.clone(),
            properties: self.properties.clone(),
        })
    }
}

pub struct FrameSnapshot(PaintFrame);

impl FrameSnapshot {
    pub fn frame(&self) -> &PaintFrame {
        &self.0
    }
}

pub(crate) fn difference(old: &[PaintInstance], current: &[PaintInstance]) -> (bool, Damage) {
    let current_keys: HashSet<_> = current.iter().map(|chunk| chunk.key).collect();
    let mut previous: HashMap<_, _> = old
        .iter()
        .filter(|chunk| current_keys.contains(&chunk.key))
        .enumerate()
        .map(|(order, chunk)| (chunk.key, (order, chunk)))
        .collect();
    let mut damage = Vec::new();
    let mut order = 0;
    let mut changed = false;
    for chunk in current {
        let before = previous.remove(&chunk.key);
        let repaint = before.is_none_or(|(old_order, old)| {
            old_order != order
                || old.origin != chunk.origin
                || old.clip != chunk.clip
                || old.program.signature() != chunk.program.signature()
        });
        changed |= repaint
            || before.is_some_and(|(_, old)| {
                old.cache != chunk.cache || old.placement != chunk.placement
            });
        if before.is_some() {
            order += 1;
        }
        if repaint {
            damage.push(chunk.visible);
            if let Some((_, old)) = before {
                damage.push(old.visible);
            }
        }
    }
    for old in old {
        if !current_keys.contains(&old.key) {
            changed = true;
            damage.push(old.visible);
        }
    }
    (changed, Damage::region(Region::new(damage)))
}

#[cfg(test)]
mod tests;
