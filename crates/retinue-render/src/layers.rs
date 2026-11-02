use super::{
    CollectionMotion, Key, PaintFrame, PaintStats, ScopeId, coverage,
    execution::{Input, Materialization, Preparation, SourceRegion, Substitute, TaskId},
    materialization::Recipe,
    residency::{Owner, RasterId, Reservation, Residency, Retention},
};
use crate::Rect;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    time::Instant,
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum CacheHint {
    #[default]
    Dynamic,
    Stable,
    Conditional,
}

#[derive(Clone, Copy, Debug)]
pub struct LayerPolicy {
    pub stable_repaints: u32,
    pub conditional_repaints: u32,
    pub min_work: usize,
    pub threads: u32,
}

impl LayerPolicy {
    pub const fn desktop() -> Self {
        Self {
            stable_repaints: 3,
            conditional_repaints: 8,
            min_work: 24,
            threads: 6,
        }
    }

    pub const fn overlay() -> Self {
        Self {
            stable_repaints: 12,
            conditional_repaints: 24,
            min_work: 64,
            threads: 2,
        }
    }
}

pub(super) struct Candidate {
    pub key: ScopeId,
    pub chunks: Range<usize>,
    pub bounds: Rect,
    pub origin: (i32, i32),
    recipe: Recipe,
    required: u32,
}

struct Entry {
    recipe: Recipe,
    raster: Option<RasterId>,
    dirty: bool,
    repaints: u32,
    cached_repaints: u32,
    last_draw: u64,
    cooldown: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Prepared {
    pub chunks: Range<usize>,
    pub bounds: Rect,
    pub origin: (i32, i32),
    pub raster: RasterId,
    pub regions: Vec<Rect>,
}

pub(super) struct Pending {
    group: CandidateOrder,
    reservation: Option<Reservation>,
}

type CandidateOrder = (usize, Reverse<usize>, ScopeId);

fn remainder(range: &Range<usize>, bounds: Rect, masks: &[(Range<usize>, Rect)]) -> Vec<Rect> {
    coverage::remaining(
        bounds,
        masks.iter().filter_map(|(r, rect)| {
            (r.start <= range.start && r.end >= range.end).then_some(*rect)
        }),
    )
}

#[derive(Default)]
pub(super) struct Layers {
    pub policy: Option<LayerPolicy>,
    entries: HashMap<ScopeId, Entry>,
    candidates: BTreeMap<CandidateOrder, Candidate>,
    scopes: HashMap<Key, Vec<CandidateOrder>>,
    pub prepared: BTreeMap<ScopeId, Prepared>,
    parents: Vec<Option<usize>>,
    nearest: Vec<Option<usize>>,
    reordering: BTreeMap<usize, Range<usize>>,
    frame: u64,
}

impl Layers {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.candidates.clear();
        self.scopes.clear();
        self.prepared.clear();
        self.parents.clear();
        self.nearest.clear();
        self.reordering.clear();
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.prepared.len()
    }

    pub fn bytes(&self, residency: &Residency) -> usize {
        self.prepared
            .values()
            .map(|p| residency.image(p.raster).byte_len())
            .sum()
    }

    fn rebuild_index(&mut self, frame: &PaintFrame) {
        self.parents = vec![None; frame.groups.len()];
        self.nearest.clear();
        let mut stack: Vec<usize> = Vec::new();
        let mut group = 0;
        for index in 0..frame.chunks.len() {
            while stack
                .last()
                .is_some_and(|&g| frame.groups[g].instances.end <= index)
            {
                stack.pop();
            }
            while group < frame.groups.len() && frame.groups[group].instances.start <= index {
                if frame.groups[group].instances.end > index {
                    self.parents[group] = stack.last().copied();
                    stack.push(group);
                }
                group += 1;
            }
            self.nearest.push(stack.last().copied());
        }
    }

    pub fn update(
        &mut self,
        frame: &PaintFrame,
        changed: &[usize],
        changed_groups: &[usize],
        full: bool,
        residency: &mut Residency,
        now: Instant,
        stats: &mut PaintStats,
    ) {
        self.frame = self.frame.wrapping_add(1);
        let Some(policy) = self.policy else { return };
        if full {
            self.reordering.clear();
        }
        let context_changes = if full {
            frame.groups.len()
        } else {
            changed_groups.len()
        };
        for offset in 0..context_changes {
            let index = if full { offset } else { changed_groups[offset] };
            let group = &frame.groups[index];
            if !group.instances.is_empty()
                && group
                    .collection()
                    .is_some_and(|c| c.motion == CollectionMotion::Reordering)
            {
                self.reordering.insert(index, group.instances.clone());
            } else {
                self.reordering.remove(&index);
            }
        }
        let affected = if full {
            self.rebuild_index(frame);
            self.candidates.clear();
            let keys: HashSet<_> = frame.groups.iter().map(|g| g.key).collect();
            self.scopes.retain(|key, _| keys.contains(key));
            self.entries.retain(|key, entry| {
                if keys.contains(&key.group) {
                    return true;
                }
                if let Some(id) = entry.raster {
                    residency.release(id, Retention::Discard, now);
                }
                false
            });
            (0..frame.groups.len()).collect::<Vec<_>>()
        } else {
            let mut affected = HashSet::new();
            let roots = changed
                .iter()
                .filter_map(|&index| self.nearest.get(index).copied().flatten())
                .chain(changed_groups.iter().copied());
            for root in roots {
                let mut group = Some(root);
                while let Some(g) = group {
                    if !affected.insert(g) {
                        break;
                    }
                    group = self.parents[g];
                }
            }
            for &index in changed_groups {
                let range = &frame.groups[index].instances;
                for child in index + 1..frame.groups.len() {
                    if frame.groups[child].instances.start >= range.end {
                        break;
                    }
                    affected.insert(child);
                }
            }
            let mut affected: Vec<_> = affected.into_iter().collect();
            affected.sort_unstable();
            affected
        };
        stats.inspected_groups += affected.len();
        for index in affected {
            let group = &frame.groups[index];
            let previous = self.scopes.remove(&group.key).unwrap_or_default();
            for order in &previous {
                self.candidates.remove(order);
            }
            let coverage = if group.cache_hint() == CacheHint::Dynamic
                || self.reordering.values().any(|range| {
                    range.start >= group.instances.start && range.end <= group.instances.end
                }) {
                Vec::new()
            } else {
                coverage::candidates(frame, group)
            };
            stats.inspected_regions += coverage.len();
            let mut orders = Vec::new();
            for region in coverage {
                let Some(recipe) = Recipe::region(
                    frame,
                    region.instances.clone(),
                    region.origin,
                    (region.content.width, region.content.height),
                ) else {
                    continue;
                };
                let candidate = Candidate {
                    key: region.id,
                    chunks: region.instances,
                    bounds: region.visible,
                    origin: region.origin,
                    recipe,
                    required: match group.cache_hint() {
                        CacheHint::Stable => policy.stable_repaints,
                        CacheHint::Conditional => policy.conditional_repaints,
                        CacheHint::Dynamic => unreachable!(),
                    },
                };
                let entry = self.entries.entry(candidate.key).or_insert_with(|| Entry {
                    raster: residency.acquire(Owner::Group(candidate.key), &candidate.recipe),
                    recipe: candidate.recipe.clone(),
                    dirty: false,
                    repaints: 0,
                    cached_repaints: 0,
                    last_draw: 0,
                    cooldown: 0,
                });
                if entry.recipe != candidate.recipe {
                    let repairable = entry.raster.is_some_and(|id| {
                        residency.materialization(id).recipe.size == candidate.recipe.size
                            && (entry.cached_repaints >= candidate.required
                                || (region.contextual
                                    && entry.recipe.same_content(&candidate.recipe)))
                    });
                    if entry.raster.is_some()
                        && !repairable
                        && entry.cached_repaints < candidate.required
                    {
                        entry.cooldown = candidate.required.saturating_mul(4);
                    }
                    entry.dirty = repairable;
                    if !repairable {
                        if let Some(id) = entry.raster.take() {
                            residency.release(id, Retention::Discard, now);
                        }
                        entry.repaints = 0;
                        entry.cached_repaints = 0;
                    }
                    entry.recipe = candidate.recipe.clone();
                }
                let order = (
                    candidate.chunks.start,
                    Reverse(candidate.chunks.end),
                    candidate.key,
                );
                orders.push(order);
                self.candidates.insert(order, candidate);
            }
            let current: HashSet<_> = orders.iter().map(|order| order.2).collect();
            for old in previous {
                if current.contains(&old.2) {
                    continue;
                }
                if let Some(entry) = self.entries.remove(&old.2) {
                    if let Some(id) = entry.raster {
                        let nearby = group.cache_hint() != CacheHint::Dynamic
                            && coverage::is_hidden(frame, group, old.2);
                        residency.release(
                            id,
                            if nearby {
                                Retention::Nearby
                            } else {
                                Retention::Discard
                            },
                            now,
                        );
                    }
                }
            }
            self.scopes.insert(group.key, orders);
        }
    }

    pub fn begin_preparations(
        &mut self,
        residency: &mut Residency,
        items: &[Option<RasterId>],
        now: Instant,
        stats: &mut PaintStats,
    ) -> Vec<Pending> {
        let Some(policy) = self.policy else {
            return Vec::new();
        };
        let mut pending = Vec::new();
        let mut reserved: Vec<(Range<usize>, Rect)> = Vec::new();
        let mut promoted = false;
        for repairing in [true, false] {
            let mut covered = reserved.clone();
            for (&group, candidate) in &self.candidates {
                if remainder(&candidate.chunks, candidate.bounds, &covered).is_empty() {
                    continue;
                }
                let entry = self.entries.get_mut(&candidate.key).unwrap();
                if entry.raster.is_some() && !entry.dirty {
                    covered.push((candidate.chunks.clone(), candidate.bounds));
                    continue;
                }
                if entry.dirty != repairing
                    || (!repairing && promoted)
                    || (!repairing
                        && reserved.iter().any(|(r, rect)| {
                            r.start < candidate.chunks.end
                                && candidate.chunks.start < r.end
                                && !rect.intersection(candidate.bounds).is_empty()
                        }))
                {
                    continue;
                }
                if (!entry.dirty && entry.repaints < candidate.required) || entry.cooldown != 0 {
                    continue;
                }
                let work: usize = candidate
                    .recipe
                    .fragments
                    .iter()
                    .enumerate()
                    .map(|(offset, f)| {
                        if f.bounds().is_empty() {
                            0
                        } else if items
                            .get(candidate.chunks.start + offset)
                            .is_some_and(Option::is_some)
                        {
                            1
                        } else {
                            f.program.summary().work
                        }
                    })
                    .sum();
                let bytes = (candidate.recipe.size.0 as usize)
                    .saturating_mul(candidate.recipe.size.1 as usize)
                    .saturating_mul(4);
                if !entry.dirty && work < policy.min_work.max(bytes / (128 * 1024)) {
                    continue;
                }
                let needs_output = entry.raster.is_none_or(|id| {
                    !candidate
                        .recipe
                        .repair(Some(&residency.materialization(id).recipe))
                        .regions
                        .is_empty()
                });
                let mut reservation = needs_output
                    .then(|| residency.reserve(&candidate.recipe))
                    .flatten();
                if needs_output && reservation.is_none() && entry.dirty {
                    // An obsolete repair donor must not permanently prevent its replacement.
                    if let Some(id) = entry.raster.take() {
                        residency.release(id, Retention::Discard, now);
                    }
                    entry.dirty = false;
                    reservation = residency.reserve(&candidate.recipe);
                }
                if !needs_output || reservation.is_some() {
                    pending.push(Pending { group, reservation });
                    reserved.push((candidate.chunks.clone(), candidate.bounds));
                    covered.push((candidate.chunks.clone(), candidate.bounds));
                    promoted |= !repairing;
                    continue;
                }
                stats.rejected_rasters += 1;
            }
        }
        pending
    }

    pub fn preparation_range(&self, pending: &Pending) -> Range<usize> {
        self.candidates[&pending.group].chunks.clone()
    }

    pub fn preparation_bounds(&self, pending: &Pending) -> Rect {
        self.candidates[&pending.group].bounds
    }

    pub fn plan(
        &self,
        pending: Pending,
        plan: &mut Preparation,
        items: &HashMap<usize, Input>,
        residency: &Residency,
    ) -> (ScopeId, TaskId) {
        let candidate = &self.candidates[&pending.group];
        let entry = &self.entries[&candidate.key];
        let mut substitutes = Vec::new();
        let mut index = candidate.chunks.start;
        while index < candidate.chunks.end {
            let child = self.candidates.values().find(|child| {
                child.key != candidate.key
                    && child.chunks.start == index
                    && child.chunks.end <= candidate.chunks.end
                    && !child.bounds.intersection(candidate.bounds).is_empty()
                    && self
                        .entries
                        .get(&child.key)
                        .is_some_and(|e| e.raster.is_some() && !e.dirty)
            });
            if let Some(child) = child {
                let end = child.chunks.end;
                let mut regions: Vec<SourceRegion> = Vec::new();
                for child in self
                    .candidates
                    .values()
                    .filter(|child| child.key != candidate.key && child.chunks == (index..end))
                {
                    let Some(raster) = self
                        .entries
                        .get(&child.key)
                        .filter(|e| !e.dirty)
                        .and_then(|e| e.raster)
                    else {
                        continue;
                    };
                    let Some(origin) = child
                        .origin
                        .0
                        .checked_sub(candidate.origin.0)
                        .zip(child.origin.1.checked_sub(candidate.origin.1))
                    else {
                        continue;
                    };
                    let clip =
                        coverage::local_clip(child.bounds, candidate.origin, candidate.recipe.size);
                    for clip in coverage::remaining(clip, regions.iter().map(|r| r.clip)) {
                        regions.push(SourceRegion {
                            source: Input::Ready(residency.image(raster).clone()),
                            origin,
                            clip,
                        });
                    }
                }
                substitutes.push(Substitute {
                    fragments: (index - candidate.chunks.start)..(end - candidate.chunks.start),
                    regions,
                });
                index = end;
            } else {
                if let Some(source) = items.get(&index) {
                    let fragment = &candidate.recipe.fragments[index - candidate.chunks.start];
                    let bounds = fragment.program.bounds();
                    substitutes.push(Substitute {
                        fragments: (index - candidate.chunks.start)
                            ..(index + 1 - candidate.chunks.start),
                        regions: vec![SourceRegion {
                            source: source.clone(),
                            origin: (fragment.origin.0 + bounds.x, fragment.origin.1 + bounds.y),
                            clip: fragment.clip,
                        }],
                    });
                }
                index += 1;
            }
        }
        let task = plan.add(
            candidate.recipe.clone(),
            entry.raster.map(|id| residency.materialization(id)),
            substitutes,
            self.policy.unwrap().threads,
        );
        if let Some(reservation) = pending.reservation {
            plan.reserve(task, reservation);
        }
        (candidate.key, task)
    }

    pub fn publish(
        &mut self,
        plan: &mut Preparation,
        results: &[Option<Materialization>],
        scheduled: &[(ScopeId, TaskId)],
        residency: &mut Residency,
        now: Instant,
    ) -> (usize, usize) {
        let mut promoted = 0;
        let mut demoted = 0;
        for &(key, task) in scheduled {
            plan.complete(task);
            if plan.deferred(task) {
                continue;
            }
            let entry = self.entries.get_mut(&key).unwrap();
            let previous = entry.raster.take();
            entry.raster = plan
                .result(task, results)
                .cloned()
                .map(|raster| residency.insert(Owner::Group(key), raster, plan.elapsed(task)));
            if let Some(id) = previous {
                residency.release(id, Retention::Discard, now);
            }
            entry.dirty = false;
            if entry.raster.is_some() {
                promoted += 1;
            } else {
                let required = self
                    .candidates
                    .values()
                    .find(|c| c.key == key)
                    .unwrap()
                    .required;
                entry.cooldown = required.max(1).saturating_mul(4);
            }
        }
        self.prepared.clear();
        let mut covered = Vec::new();
        for candidate in self.candidates.values() {
            let entry = self.entries.get_mut(&candidate.key).unwrap();
            let regions = remainder(&candidate.chunks, candidate.bounds, &covered);
            if regions.is_empty() {
                if let Some(id) = entry.raster.take() {
                    residency.release(id, Retention::Discard, now);
                    demoted += 1;
                }
                entry.dirty = false;
                continue;
            }
            if let Some(raster) = entry.raster.filter(|_| !entry.dirty) {
                self.prepared.insert(
                    candidate.key,
                    Prepared {
                        chunks: candidate.chunks.clone(),
                        bounds: candidate.bounds,
                        origin: candidate.origin,
                        raster,
                        regions: regions.clone(),
                    },
                );
                covered.extend(regions.into_iter().map(|r| (candidate.chunks.clone(), r)));
            }
        }
        (promoted, demoted)
    }

    pub fn drawn(&mut self, repair: &crate::damage::Region, residency: &mut Residency) {
        for candidate in self.candidates.values() {
            let area = |r: Rect| u64::from(r.width) * u64::from(r.height);
            let repaired: u64 = repair
                .rects()
                .iter()
                .map(|r| area(candidate.bounds.intersection(*r)))
                .sum();
            if repaired * 2 < area(candidate.bounds) {
                continue;
            }
            let entry = self.entries.get_mut(&candidate.key).unwrap();
            if entry.last_draw == self.frame {
                continue;
            }
            entry.last_draw = self.frame;
            entry.repaints = entry.repaints.saturating_add(1);
            entry.cooldown = entry.cooldown.saturating_sub(1);
            entry.cached_repaints = entry
                .cached_repaints
                .saturating_add(u32::from(entry.raster.is_some() && !entry.dirty));
            if let Some(id) = entry.raster.filter(|_| !entry.dirty) {
                residency.drawn(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{painter::Painter, *};
    use std::rc::Rc;

    fn policy() -> LayerPolicy {
        LayerPolicy {
            stable_repaints: 2,
            conditional_repaints: 5,
            min_work: 1,
            threads: 2,
        }
    }

    fn scene(scale: f32, hints: (CacheHint, CacheHint)) -> PaintFrame {
        let mut instances = Vec::new();
        let mut groups = Vec::new();
        let mut translations = vec![Translation {
            id: TransformId(1.into()),
            parent: None,
            offset: (10.0, 10.0),
        }];
        if hints.0 != CacheHint::Dynamic {
            groups.push(group(1, 0..2, hints.0));
        }
        for i in 0..2 {
            let key = Key::from(i as u64 + 2);
            let program = PaintProgram::new(
                PaintKey::new(key, 0),
                (25.0, 30.0),
                scale,
                Rc::new(vec![Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 25.0, 30.0),
                    radius: 4.0,
                    color: Color::rgba(100, (i * 80) as u8, 150, 190),
                }]),
            )
            .unwrap();
            translations.push(Translation {
                id: TransformId(key),
                parent: Some(TransformId(1.into())),
                offset: (i as f32 * 30.0, 5.0),
            });
            let mut instance = PaintInstance::with_properties(
                key,
                program,
                PropertyBinding {
                    transform: Some(TransformId(key)),
                    clip: None,
                },
            );
            instance.cache = CachePolicy::Prefer;
            instances.push(instance);
            if hints.1 != CacheHint::Dynamic {
                groups.push(group(i as u64 + 2, i..i + 1, hints.1));
            }
        }
        let mut scene = PaintFrame::default();
        scene
            .replace_with_properties(
                (240, 200),
                scale,
                instances,
                groups,
                PropertyDefinitions {
                    translations,
                    clips: vec![],
                },
            )
            .unwrap();
        scene
    }

    fn group(key: u64, instances: Range<usize>, hint: CacheHint) -> PaintGroup {
        PaintGroup {
            key: key.into(),
            instances,
            context: OptimizationContext::default().with(hint),
        }
    }

    fn record(frame: &mut PaintFrame, key: Key, commands: Vec<Command>, cache: CachePolicy) {
        let index = frame
            .instances()
            .iter()
            .position(|instance| instance.key == key)
            .unwrap();
        let old = &frame.instances()[index].program;
        let program = PaintProgram::new(
            PaintKey::new(key, old.key().revision + 1),
            old.size(),
            frame.scale(),
            Rc::new(commands),
        )
        .unwrap();
        frame
            .patch(
                [PaintUpdate {
                    index,
                    program,
                    cache,
                }],
                [],
            )
            .unwrap();
    }

    fn offset(frame: &mut PaintFrame, offset: (f32, f32)) {
        frame
            .patch_properties([PropertyChange::Translation(
                TransformId(1.into()),
                (10.0 + offset.0, 10.0 + offset.1),
            )])
            .unwrap();
    }

    fn paint(scene: &PaintFrame, renderer: &mut Renderer, repair: Rect) -> Vec<u8> {
        let mut pixels = vec![0; 240 * 200 * 4];
        renderer.prepare(scene, RasterMode::Explicit);
        let mut p = Painter::new(&mut pixels, 240, 200).unwrap();
        renderer.paint(scene, &mut p, repair, Color::rgb(0x19283d));
        p.finish().unwrap();
        pixels
    }

    fn full() -> Rect {
        Rect {
            width: 240,
            height: 200,
            ..Rect::default()
        }
    }

    #[test]
    fn promotion_requires_repaints_not_idle_prepares() {
        let scene = scene(1.0, (CacheHint::Stable, CacheHint::Dynamic));
        let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(32768), policy());
        for _ in 0..20 {
            r.prepare(&scene, RasterMode::Explicit);
        }
        assert_eq!(r.layers.count(), 0);
        paint(&scene, &mut r, full());
        paint(&scene, &mut r, full());
        paint(&scene, &mut r, full());
        assert_eq!(r.stats.promoted_layers, 1);
        assert_eq!(r.stats.blits, 1);
        assert_eq!(r.residency_stats().live_bytes, r.layers.bytes(&r.residency));
    }

    #[test]
    fn cached_layers_match_replay_through_motion_content_clips_and_scale() {
        let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(32768), policy());
        let mut oracle = Renderer::new(crate::ResidencyPolicy::desktop(0));
        oracle.set_optimizations(false);
        for scale in [1.0, 1.25, 2.0] {
            let mut scene = scene(scale, (CacheHint::Stable, CacheHint::Dynamic));
            for frame in 0..18 {
                if frame == 6 {
                    record(
                        &mut scene,
                        2.into(),
                        vec![Command::Fill {
                            bounds: Bounds::new(-3.0, 2.0, 28.0, 30.0),
                            radius: 6.0,
                            color: Color::rgba(20, 90, 255, 128),
                        }],
                        CachePolicy::Prefer,
                    );
                }
                offset(
                    &mut scene,
                    (
                        if frame < 10 {
                            frame as f32 * 0.8
                        } else {
                            -(frame as f32) * 2.5
                        },
                        frame as f32 * 0.35,
                    ),
                );
                let repair = if frame % 4 == 0 {
                    Rect {
                        x: 12,
                        y: 8,
                        width: 23,
                        height: 40,
                    }
                } else {
                    full()
                };
                let actual = paint(&scene, &mut r, repair);
                let expected = paint(&scene, &mut oracle, repair);
                let max = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(max <= 2, "scale={scale} frame={frame} error={max}");
            }
        }
    }

    #[test]
    fn parent_promotion_releases_child_layers_and_hiding_releases_parent() {
        let mut scene = scene(1.0, (CacheHint::Conditional, CacheHint::Stable));
        let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(32768), policy());
        for _ in 0..5 {
            paint(&scene, &mut r, full());
        }
        assert_eq!(r.layers.count(), 2);
        paint(&scene, &mut r, full());
        assert_eq!(r.stats.promoted_layers, 1);
        assert_eq!(r.stats.demoted_layers, 2);
        assert_eq!(r.layers.count(), 1);
        assert_eq!(r.stats.blits, 1);
        offset(&mut scene, (400.0, 0.0));
        paint(&scene, &mut r, full());
        assert_eq!(r.layers.count(), 0);
        assert_eq!(r.layers.bytes(&r.residency), 0);
    }

    #[test]
    fn changing_layers_cool_down_and_dynamic_or_external_paint_never_promotes() {
        let mut scene = scene(1.0, (CacheHint::Stable, CacheHint::Dynamic));
        let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(32768), policy());
        for _ in 0..3 {
            paint(&scene, &mut r, full());
        }
        assert_eq!(r.layers.count(), 1);
        record(
            &mut scene,
            2.into(),
            vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 25.0, 30.0),
                radius: 0.0,
                color: Color::rgb(0x112233),
            }],
            CachePolicy::Prefer,
        );
        for _ in 0..4 {
            paint(&scene, &mut r, full());
        }
        assert_eq!(r.layers.count(), 0);
        record(
            &mut scene,
            2.into(),
            vec![Command::External {
                key: 2.into(),
                bounds: Bounds::new(0.0, 0.0, 25.0, 30.0),
            }],
            CachePolicy::Never,
        );
        r.prepare(&scene, RasterMode::Explicit);
        assert!(r.layers.candidates.is_empty());
        r.clear_cache();
        assert_eq!(r.residency_stats().live_bytes, 0);
        let scene = super::tests::scene(1.0, (CacheHint::Dynamic, CacheHint::Dynamic));
        for _ in 0..8 {
            paint(&scene, &mut r, full());
        }
        assert!(r.layers.candidates.is_empty());
    }

    #[test]
    fn layer_budget_and_disabled_optimization_preserve_direct_paint() {
        let scene = scene(1.0, (CacheHint::Stable, CacheHint::Dynamic));
        let mut small = crate::ResidencyPolicy::desktop(0);
        small.peak_bytes = 1024;
        let mut r = Renderer::with_layers(small, policy());
        for _ in 0..8 {
            paint(&scene, &mut r, full());
        }
        assert_eq!(r.layers.count(), 0);
        let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(0), policy());
        for _ in 0..3 {
            paint(&scene, &mut r, full());
        }
        assert_eq!(r.layers.count(), 1);
        r.set_optimizations(false);
        paint(&scene, &mut r, full());
        assert_eq!(r.layers.count(), 0);
        assert_eq!(r.stats.blits, 0);
        assert!(r.stats.replayed_commands > 0);
        assert!(
            crate::ResidencyPolicy::overlay(0).peak_bytes
                < crate::ResidencyPolicy::desktop(0).peak_bytes
        );
    }

    #[test]
    fn promotion_preserves_a_child_raster_used_outside_the_group() {
        let drawing = PaintProgram::new(
            8,
            (20.0, 20.0),
            1.0,
            Rc::new(vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 20.0, 20.0),
                radius: 3.0,
                color: Color::rgb(0x224466),
            }]),
        )
        .unwrap();
        let leaf = |key, x| {
            let mut e = PaintInstance::new(key, drawing.clone(), (x, 0), full());
            e.cache = CachePolicy::Prefer;
            e
        };
        let mut scene = PaintFrame::default();
        scene
            .replace(
                (240, 200),
                1.0,
                vec![leaf(2.into(), 0), leaf(3.into(), 80)],
                vec![group(1, 0..1, CacheHint::Stable)],
            )
            .unwrap();
        let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(32768), policy());
        for _ in 0..3 {
            paint(&scene, &mut r, full());
        }
        assert_eq!(r.layers.count(), 1);
        assert!(r.residency_stats().live_bytes > r.layers.bytes(&r.residency));
        assert_eq!(r.stats.blits, 2);
    }

    #[test]
    fn fractional_polygon_edges_survive_promotion() {
        for scale in [1.0, 1.25, 2.0] {
            let drawing = PaintProgram::new(
                1,
                (40.0, 40.0),
                scale,
                Rc::new(vec![Command::Polygon {
                    points: vec![[1.2, 1.2], [20.2, 1.2], [20.2, 20.2], [1.2, 20.2]],
                    color: Color::rgba(80, 100, 150, 190),
                }]),
            )
            .unwrap();
            let origin = (10.0_f32 * scale).round() as i32;
            let item = PaintInstance::new(1.into(), drawing, (origin, origin), full());
            let mut scene = PaintFrame::default();
            scene
                .replace(
                    (240, 200),
                    scale,
                    vec![item],
                    vec![group(1, 0..1, CacheHint::Stable)],
                )
                .unwrap();
            let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(0), policy());
            let mut oracle = Renderer::new(crate::ResidencyPolicy::desktop(0));
            oracle.set_optimizations(false);
            let expected = paint(&scene, &mut oracle, full());
            for _ in 0..3 {
                let actual = paint(&scene, &mut r, full());
                let max = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(max <= 1, "scale={scale} error={max}");
            }
            assert_eq!(r.layers.count(), 1);
        }
    }

    #[test]
    fn tiny_repairs_do_not_justify_a_whole_layer() {
        let scene = scene(1.0, (CacheHint::Stable, CacheHint::Dynamic));
        let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(0), policy());
        for _ in 0..20 {
            paint(
                &scene,
                &mut r,
                Rect {
                    x: 12,
                    y: 17,
                    width: 2,
                    height: 2,
                },
            );
        }
        assert_eq!(r.layers.count(), 0);
    }

    #[test]
    fn mature_parent_repairs_small_changes_while_moving_without_rebuilding_children() {
        for scale in [1.0, 1.25, 2.0] {
            let mut scene = scene(scale, (CacheHint::Stable, CacheHint::Dynamic));
            let commands = |color| {
                vec![
                    Command::Fill {
                        bounds: Bounds::new(0.0, 0.0, 25.0, 30.0),
                        radius: 4.0,
                        color: Color::rgba(100, 80, 150, 190),
                    },
                    Command::Fill {
                        bounds: Bounds::new(9.0, 7.0, 3.0, 3.0),
                        radius: 0.0,
                        color: Color::rgb(color),
                    },
                ]
            };
            record(
                &mut scene,
                2.into(),
                commands(0xabcdef),
                CachePolicy::Prefer,
            );
            let mut r = Renderer::with_layers(crate::ResidencyPolicy::desktop(32768), policy());
            for _ in 0..6 {
                paint(&scene, &mut r, full());
            }
            let previous = r
                .residency
                .image(r.layers.entries[&ScopeId::group(1.into())].raster.unwrap())
                .clone();
            let previous_pixels = previous.to_rgba().unwrap();
            record(
                &mut scene,
                2.into(),
                commands(0x123456),
                CachePolicy::Prefer,
            );
            offset(&mut scene, (1.6, 2.4));
            let actual = paint(&scene, &mut r, full());
            let mut direct = Renderer::new(crate::ResidencyPolicy::desktop(0));
            direct.set_optimizations(false);
            let expected = paint(&scene, &mut direct, full());
            let error = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(error <= 2, "scale={scale} error={error}");
            assert_eq!(r.layers.count(), 1);
            assert_eq!(r.stats.prepared_rasters, 0);
            assert_eq!(r.stats.preparation_tasks, 1);
            assert!(r.stats.repaired_pixels < r.stats.copied_pixels / 10);
            assert_eq!(previous.to_rgba().unwrap(), previous_pixels);
            assert_ne!(
                r.residency
                    .image(r.layers.prepared[&ScopeId::group(1.into())].raster)
                    .id(),
                previous.id()
            );
        }
    }
}
