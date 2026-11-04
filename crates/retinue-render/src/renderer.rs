use super::{
    CachePolicy, Color, DrawStep, Key, PaintFrame, PaintStats, RasterMode, RenderHost,
    ResidencyPolicy, ResidencyStats, ResourceRead, ScopeId,
    dependencies::Dependencies,
    execution::{Input, PaintDemand, Preparation},
    fragments::{self, Fragments},
    layers::{Layers, Prepared},
    materialization::Recipe,
    residency::{Owner, RasterId, Residency, Retention},
};
use crate::{Rect, painter::Painter};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    rc::Rc,
    time::Instant,
};

pub(crate) mod cost;
mod coverage;
use coverage::SpatialIndex;

type DrawOrder = (usize, u8, ScopeId);

fn instance_order(index: usize) -> DrawOrder {
    (index, 0, ScopeId::group(0.into()))
}

pub struct Renderer {
    dependencies: Dependencies,
    pub(super) residency: Residency,
    prepared: Vec<Option<RasterId>>,
    prepared_for: Option<(Rc<()>, u64)>,
    draw_plan: BTreeMap<DrawOrder, DrawStep>,
    coverage: SpatialIndex,
    retry_items: BTreeSet<usize>,
    unrequested_items: BTreeSet<usize>,
    blocked_items: BTreeSet<usize>,
    analysis_retry: BTreeSet<usize>,
    reclaimed: u64,
    mode: RasterMode,
    optimizations: bool,
    materializations: bool,
    deferred: bool,
    pub(super) layers: Layers,
    pub(super) fragments: Fragments,
    pub stats: PaintStats,
}

impl Renderer {
    pub fn new(policy: ResidencyPolicy) -> Self {
        let optimizations = true;
        #[cfg(feature = "benchmark")]
        let optimizations =
            optimizations && std::env::var_os("RETINUE_BENCH_FULL_REPLAY").is_none();
        Self {
            dependencies: Default::default(),
            residency: Residency::new(policy),
            prepared: Vec::new(),
            prepared_for: None,
            draw_plan: BTreeMap::new(),
            coverage: SpatialIndex::default(),
            retry_items: BTreeSet::new(),
            unrequested_items: BTreeSet::new(),
            blocked_items: BTreeSet::new(),
            analysis_retry: BTreeSet::new(),
            reclaimed: 0,
            mode: RasterMode::Replay,
            optimizations,
            materializations: true,
            deferred: false,
            layers: Default::default(),
            fragments: Default::default(),
            stats: Default::default(),
        }
    }

    pub fn with_layers(policy: ResidencyPolicy, layers: super::LayerPolicy) -> Self {
        let mut renderer = Self::new(policy);
        let enabled = true;
        #[cfg(feature = "benchmark")]
        let enabled = enabled && std::env::var_os("RETINUE_BENCH_NO_LAYERS").is_none();
        if enabled {
            renderer.layers.policy = Some(layers);
        }
        renderer
    }

    pub fn set_optimizations(&mut self, enabled: bool) {
        if enabled != self.optimizations {
            self.clear_cache();
        }
        self.optimizations = enabled;
    }

    pub fn set_host(&mut self, host: RenderHost) {
        self.clear_cache();
        self.residency = Residency::with_host(self.residency.policy, host);
    }

    pub fn host(&self) -> &RenderHost {
        &self.residency.host
    }

    pub fn has_deferred(&self) -> bool {
        self.deferred
    }

    pub fn set_materializations(&mut self, enabled: bool) {
        if self.materializations != enabled {
            self.clear_cache();
        }
        self.materializations = enabled;
    }

    pub fn clear_cache(&mut self) {
        self.dependencies.clear();
        self.prepared.clear();
        self.layers.clear();
        self.fragments = Fragments::default();
        self.draw_plan.clear();
        self.coverage.clear();
        self.retry_items.clear();
        self.unrequested_items.clear();
        self.blocked_items.clear();
        self.analysis_retry.clear();
        self.prepared_for = None;
        self.deferred = false;
        self.residency.clear();
    }

    pub fn trim_inactive(&mut self) {
        self.residency.trim_warm();
    }

    pub fn execution_plan(&self) -> impl ExactSizeIterator<Item = &DrawStep> {
        self.draw_plan.values()
    }

    pub fn resource_consumers(&self, resource: ResourceRead) -> impl Iterator<Item = Key> + '_ {
        self.dependencies.consumers(resource)
    }

    pub fn residency_stats(&self) -> ResidencyStats {
        self.residency.stats()
    }

    fn refresh_coverage(&mut self) {
        self.coverage.update(
            self.layers
                .prepared
                .values()
                .map(|p| (p.chunks.clone(), p.regions.clone())),
        );
        self.draw_plan.retain(|(_, phase, _), _| *phase == 0);
        for (&group, p) in &self.layers.prepared {
            self.draw_plan.insert(
                (p.chunks.end - 1, 1, group),
                DrawStep::Layer {
                    group,
                    instances: p.chunks.clone(),
                },
            );
        }
    }

    fn coverage_changes(
        before: &BTreeMap<ScopeId, Prepared>,
        after: &BTreeMap<ScopeId, Prepared>,
        changed: &mut BTreeSet<usize>,
    ) {
        for (key, old) in before {
            if after
                .get(key)
                .is_none_or(|new| old.chunks != new.chunks || old.regions != new.regions)
            {
                changed.extend(old.chunks.clone());
            }
        }
        for (key, new) in after {
            if before
                .get(key)
                .is_none_or(|old| old.chunks != new.chunks || old.regions != new.regions)
            {
                changed.extend(new.chunks.clone());
            }
        }
    }

    pub fn prepare(&mut self, frame: &PaintFrame, mode: RasterMode) {
        self.prepare_with_demand(frame, mode, PaintDemand::Full);
    }

    /// Limit optional preparation to repairs without removing retained consumers.
    pub fn prepare_for(
        &mut self,
        frame: &PaintFrame,
        mode: RasterMode,
        repair: &crate::damage::Region,
    ) {
        self.prepare_with_demand(frame, mode, PaintDemand::Repair(repair));
    }

    fn prepare_with_demand(
        &mut self,
        frame: &PaintFrame,
        mode: RasterMode,
        demand: PaintDemand<'_>,
    ) {
        let host = self.residency.host.clone();
        let _pass = host.begin_pass(0);
        self.stats = PaintStats::default();
        let now = Instant::now();
        if self
            .prepared_for
            .as_ref()
            .is_some_and(|(id, _)| !Rc::ptr_eq(id, &frame.identity))
        {
            self.clear_cache();
        }
        self.residency.maintain(now);
        if self.residency.reclaimed() != self.reclaimed {
            self.retry_items.append(&mut self.blocked_items);
            self.reclaimed = self.residency.reclaimed();
        }
        self.dependencies.update(frame);
        if self.dependencies.full {
            self.analysis_retry.clear();
        }
        if self.optimizations {
            self.analysis_retry.extend(&self.dependencies.changed);
            let pending: Vec<_> = self.analysis_retry.iter().copied().take(32).collect();
            for index in pending {
                if host.analyze(&frame.chunks[index].program) {
                    self.analysis_retry.remove(&index);
                } else {
                    break;
                }
            }
        }
        self.stats.inspected_instances = self.dependencies.visited;
        self.stats.changed_programs = self.dependencies.paints;
        let full = self.dependencies.full || self.mode != mode || self.prepared_for.is_none();
        if full {
            self.blocked_items.clear();
            self.unrequested_items.clear();
        }
        let unchanged = self.mode == mode
            && self.prepared_for.as_ref().is_some_and(|(id, revision)| {
                Rc::ptr_eq(id, &frame.identity) && *revision == frame.generation
            });
        self.mode = mode;
        let mut plan = Preparation::default();
        let before_layers = if !unchanged {
            self.layers.prepared.clone()
        } else {
            BTreeMap::new()
        };
        if self.optimizations && self.materializations {
            self.layers.update(
                frame,
                &self.dependencies.changed,
                &self.dependencies.groups,
                full,
                &mut self.residency,
                now,
                &mut self.stats,
            );
            if !unchanged {
                let (_, demoted) =
                    self.layers
                        .publish(&mut plan, &[], &[], &mut self.residency, now);
                self.stats.demoted_layers += demoted;
                self.refresh_coverage();
            }
        }
        let pending = if self.optimizations && self.materializations {
            self.layers.begin_preparations(
                demand,
                &mut self.residency,
                &self.prepared,
                now,
                &mut self.stats,
            )
        } else {
            Vec::new()
        };
        let requested: Vec<_> = self
            .unrequested_items
            .iter()
            .copied()
            .filter(|&index| demand.intersects(frame.chunks[index].visible))
            .collect();
        let awaiting_fragments = self.fragments.awaiting_demand && !demand.is_empty();
        if unchanged
            && pending.is_empty()
            && self.retry_items.is_empty()
            && requested.is_empty()
            && !awaiting_fragments
            && !self.deferred
        {
            self.stats.layer_bytes = self.layers.bytes(&self.residency);
            return;
        }
        let current_layers = self.layers.prepared.clone();
        let group_ranges: Vec<_> = pending
            .iter()
            .map(|p| self.layers.preparation_range(p))
            .collect();
        let mut pending_coverage = SpatialIndex::default();
        pending_coverage.update(pending.iter().map(|p| {
            (
                self.layers.preparation_range(p),
                vec![self.layers.preparation_bounds(p)],
            )
        }));
        let mut changed: BTreeSet<_> = if full {
            (0..frame.chunks.len()).collect()
        } else {
            self.dependencies.changed.iter().copied().collect()
        };
        changed.append(&mut self.retry_items);
        changed.extend(requested);
        if !unchanged {
            Self::coverage_changes(&before_layers, &current_layers, &mut changed);
        }
        changed.retain(|&i| i < frame.chunks.len());
        let previous = if full {
            self.draw_plan.retain(|(_, phase, _), _| *phase == 1);
            std::mem::replace(&mut self.prepared, vec![None; frame.chunks.len()])
        } else {
            Vec::new()
        };
        let mut retired = Vec::new();
        let mut scheduled = Vec::new();
        let fragment_tasks = if self.optimizations && self.materializations {
            let roots = self
                .dependencies
                .fragment_roots
                .iter()
                .map(|&index| (index, &frame.chunks[index]))
                .filter(|(index, chunk)| {
                    !chunk.visible.is_empty()
                        && !self.coverage.covers(*index, chunk.visible)
                        && !self.prepared[*index]
                            .is_some_and(|id| self.residency.is_item(id, chunk.program.id()))
                })
                .map(|(_, chunk)| (&chunk.program, chunk.origin, chunk.clip))
                .collect::<Vec<_>>();
            self.fragments.prepare(
                roots,
                demand,
                &mut self.residency,
                &mut plan,
                self.layers.policy.map_or(16, |p| p.min_work),
                self.layers
                    .policy
                    .unwrap_or_else(super::LayerPolicy::desktop)
                    .stable_repaints,
                now,
                &mut self.stats,
            )
        } else {
            Vec::new()
        };
        let mut tasks: HashMap<_, _> = fragment_tasks.iter().copied().collect();
        for &index in &changed {
            self.blocked_items.remove(&index);
            self.unrequested_items.remove(&index);
            let chunk = &frame.chunks[index];
            let old = self.prepared[index];
            let eligible = self.optimizations
                && self.materializations
                && mode == RasterMode::Explicit
                && chunk.cache == CachePolicy::Prefer
                && !chunk.visible.is_empty()
                && !self.coverage.covers(index, chunk.visible)
                && !chunk.program.analysis().effects.barrier;
            if eligible && old.is_some_and(|id| self.residency.is_item(id, chunk.program.id())) {
                continue;
            }
            if let Some(id) = self.prepared[index].take() {
                let retention = if chunk.visible.is_empty()
                    && chunk.cache == CachePolicy::Prefer
                    && self.residency.is_item(id, chunk.program.id())
                {
                    Retention::Nearby
                } else {
                    Retention::Discard
                };
                retired.push((id, retention));
            }
            if !eligible {
                continue;
            }
            if !demand.intersects(chunk.visible) {
                self.unrequested_items.insert(index);
                continue;
            }
            let recipe = Recipe::item(&chunk.program);
            let owner = Owner::Item(chunk.program.id());
            if let Some(id) = self.residency.acquire(owner, &recipe) {
                self.prepared[index] = Some(id);
                continue;
            }
            if pending_coverage.covers(index, chunk.visible) {
                continue;
            }
            let task = if let Some(task) = tasks.get(&chunk.program.id()).copied() {
                Some(task)
            } else {
                let previous = old
                    .map(|id| self.residency.materialization(id))
                    .filter(|raster| raster.recipe.size == recipe.size)
                    .or_else(|| self.residency.previous_item(chunk.program.signature().slot));
                let can_retry = self.residency.eligible(&recipe);
                let task = plan.admit(&mut self.residency, recipe, previous, Vec::new(), 1);
                if let Some(task) = task {
                    tasks.insert(chunk.program.id(), task);
                } else {
                    self.stats.rejected_rasters += 1;
                    if can_retry {
                        self.blocked_items.insert(index);
                    }
                }
                task
            };
            if let Some(task) = task {
                scheduled.push((index, owner, task));
            }
        }
        let layer_tasks: Vec<_> = pending
            .into_iter()
            .map(|pending| {
                let range = self.layers.preparation_range(&pending);
                let mut sources: HashMap<_, _> = range
                    .clone()
                    .filter_map(|index| {
                        self.prepared[index]
                            .map(|id| (index, Input::Ready(self.residency.image(id).clone())))
                    })
                    .collect();
                for &(index, _, task) in &scheduled {
                    if range.contains(&index) {
                        sources.insert(index, Input::Task(task));
                    }
                }
                self.layers
                    .plan(pending, &mut plan, &sources, &self.residency)
            })
            .collect();
        let results = plan.execute_on(&host, &mut self.stats);
        self.deferred = plan.has_deferred();
        self.fragments.publish(
            &fragment_tasks,
            &mut plan,
            &results,
            &mut self.residency,
            &mut self.stats,
        );
        let mut published = HashMap::new();
        for (index, owner, task) in scheduled {
            plan.complete(task);
            if plan.deferred(task) {
                self.retry_items.insert(index);
            }
            if let Some(result) = plan.result(task, &results) {
                let id = if let Some(&id) = published.get(&task) {
                    self.residency.retain(id, &result.recipe);
                    id
                } else {
                    self.stats.prepared_rasters += usize::from(plan.rasterized(task));
                    let id = self
                        .residency
                        .acquire(owner, &result.recipe)
                        .unwrap_or_else(|| {
                            self.residency
                                .insert(owner, result.clone(), plan.elapsed(task))
                        });
                    published.insert(task, id);
                    id
                };
                self.prepared[index] = Some(id);
            }
        }
        if self.optimizations && !layer_tasks.is_empty() {
            let (promoted, demoted) =
                self.layers
                    .publish(&mut plan, &results, &layer_tasks, &mut self.residency, now);
            self.stats.promoted_layers += promoted;
            self.stats.demoted_layers += demoted;
            self.stats.layer_prepare_ns = layer_tasks
                .iter()
                .map(|(_, task)| plan.elapsed(*task))
                .sum();
            self.refresh_coverage();
            Self::coverage_changes(&current_layers, &self.layers.prepared, &mut changed);
            for ((_, task), range) in layer_tasks.iter().zip(&group_ranges) {
                if plan.result(*task, &results).is_none() {
                    self.retry_items.extend(range.clone());
                }
            }
        }
        for &index in &changed {
            if self.coverage.covers(index, frame.chunks[index].visible)
                && !frame.chunks[index].visible.is_empty()
            {
                if let Some(id) = self.prepared[index].take() {
                    retired.push((id, Retention::Discard));
                }
            }
        }
        // Rebind every surviving consumer before releasing the preceding plan.
        for id in previous.into_iter().flatten() {
            retired.push((id, Retention::Discard));
        }
        for (id, retention) in retired {
            self.residency.release(id, retention, now);
        }
        self.stats.planned_instances = changed.len();
        for index in changed {
            let visible = frame.chunks[index].visible;
            let regions = self.coverage.remaining(index, visible);
            if regions.is_empty() {
                self.draw_plan.remove(&instance_order(index));
            } else {
                let raster = self.prepared[index].is_some();
                self.draw_plan.insert(
                    instance_order(index),
                    if regions.as_slice() != [visible] {
                        DrawStep::Clipped {
                            instance: index,
                            regions,
                            raster,
                        }
                    } else if raster {
                        DrawStep::Item { instance: index }
                    } else {
                        DrawStep::Replay { instance: index }
                    },
                );
            }
        }
        self.stats.layer_bytes = self.layers.bytes(&self.residency);
        let roots = self.draw_plan.values().filter_map(|step| match step {
            DrawStep::Replay { instance }
            | DrawStep::Clipped {
                instance,
                raster: false,
                ..
            } => {
                let chunk = &frame.chunks[*instance];
                Some((&chunk.program, chunk.origin, chunk.clip))
            }
            _ => None,
        });
        self.fragments
            .retain_for_draw(roots, &mut self.residency, now);
        drop(results);
        drop(plan);
        if full {
            let mut programs = HashSet::new();
            for chunk in &frame.chunks {
                fragments::program_ids(&chunk.program, &mut programs);
            }
            let groups: HashMap<_, _> = frame.groups.iter().map(|g| (g.key, g)).collect();
            self.residency.retain_warm(|owner| match owner {
                Owner::Item(id) => programs.contains(&id),
                Owner::Group(id) => groups
                    .get(&id.group)
                    .is_some_and(|g| super::coverage::declared(g, id)),
            });
        }
        self.residency.maintain(now);
        self.prepared_for = Some((frame.identity.clone(), frame.generation));
    }

    pub fn paint(
        &mut self,
        frame: &PaintFrame,
        painter: &mut Painter<'_>,
        repair: Rect,
        background: Color,
    ) {
        self.paint_with(frame, painter, repair, background, |_, _| {
            panic!("external paint requires a host")
        });
    }

    pub fn paint_with(
        &mut self,
        frame: &PaintFrame,
        painter: &mut Painter<'_>,
        repair: Rect,
        background: Color,
        external: impl FnMut(Key, &mut Painter<'_>),
    ) {
        painter.viewport(repair, (0, 0), |painter| {
            painter.clear();
            painter.fill_rounded_rect(
                repair.x,
                repair.y,
                repair.width,
                repair.height,
                0,
                background.bgra(),
            );
            self.draw_with(frame, painter, repair, external);
        });
    }

    pub fn draw_with(
        &mut self,
        frame: &PaintFrame,
        painter: &mut Painter<'_>,
        repair: Rect,
        mut external: impl FnMut(Key, &mut Painter<'_>),
    ) {
        assert!(
            self.prepared_for
                .as_ref()
                .is_some_and(|(id, revision)| Rc::ptr_eq(id, &frame.identity)
                    && *revision == frame.generation),
            "prepare this frame generation before painting"
        );
        let repair = repair.intersection(painter.visible_bounds());
        let visible = painter.visible_regions().intersection(repair);
        self.layers.drawn(&visible, &mut self.residency);
        let mut playback = Playback {
            frame,
            prepared: &self.prepared,
            fragments: &mut self.fragments,
            residency: &mut self.residency,
            stats: &mut self.stats,
            optimizations: self.optimizations,
            sparse_repair: visible.rects().len() > 1,
        };
        painter.viewport(repair, (0, 0), |painter| {
            for step in self.draw_plan.values() {
                match step {
                    DrawStep::Layer { group, .. } => {
                        let layer = &self.layers.prepared[group];
                        for &region in &layer.regions {
                            if !painter.intersects(region.intersection(repair)) {
                                continue;
                            }
                            let image = playback.residency.image(layer.raster);
                            let (width, height) = image.dimensions();
                            painter.viewport(region, (0, 0), |painter| {
                                painter.translated(layer.origin, |painter| {
                                    painter.draw_image(image, 0, 0, width, height);
                                });
                            });
                            playback.stats.blits += 1;
                        }
                    }
                    DrawStep::Item { instance } | DrawStep::Replay { instance } => {
                        playback.instance(
                            *instance,
                            matches!(step, DrawStep::Item { .. }),
                            painter,
                            repair,
                            &mut external,
                        );
                    }
                    DrawStep::Clipped {
                        instance,
                        regions,
                        raster,
                    } => {
                        for &region in regions {
                            let clip = region.intersection(repair);
                            if clip.is_empty() {
                                continue;
                            }
                            painter.viewport(clip, (0, 0), |painter| {
                                playback.instance(*instance, *raster, painter, clip, &mut external);
                            });
                        }
                    }
                }
            }
        });
    }
}

struct Playback<'a> {
    frame: &'a PaintFrame,
    prepared: &'a [Option<RasterId>],
    fragments: &'a mut Fragments,
    residency: &'a mut Residency,
    stats: &'a mut PaintStats,
    optimizations: bool,
    sparse_repair: bool,
}

impl Playback<'_> {
    fn instance(
        &mut self,
        index: usize,
        raster: bool,
        painter: &mut Painter<'_>,
        repair: Rect,
        external: &mut impl FnMut(Key, &mut Painter<'_>),
    ) {
        let chunk = &self.frame.chunks[index];
        if !painter.intersects(chunk.visible.intersection(repair)) {
            return;
        }
        painter.viewport(chunk.clip, (0, 0), |painter| {
            if raster {
                let id = self.prepared[index].unwrap();
                self.residency.drawn(id);
                let image = self.residency.image(id);
                let bounds = chunk.program.bounds();
                painter.translated(
                    (chunk.origin.0 + bounds.x, chunk.origin.1 + bounds.y),
                    |painter| {
                        painter.draw_image(image, 0, 0, bounds.width, bounds.height);
                    },
                );
                self.stats.blits += 1;
            } else {
                let optimized = self.optimizations
                    && (self.sparse_repair || chunk.visible.intersection(repair) != chunk.visible);
                if self.optimizations {
                    self.stats.simplified_commands += chunk.program.summary().removed;
                }
                chunk.program.replay(
                    painter,
                    chunk.origin,
                    external,
                    self.optimizations,
                    optimized,
                    self.stats,
                    &mut |program, context| {
                        if !fragments::allowed(context) {
                            return None;
                        }
                        let Some(&id) = self.fragments.selected.get(&program.id()) else {
                            self.fragments.replayed(program);
                            return None;
                        };
                        self.residency.drawn(id);
                        Some(self.residency.image(id).clone())
                    },
                );
            }
        });
    }
}
