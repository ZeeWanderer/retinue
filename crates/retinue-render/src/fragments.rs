use super::{
    CacheHint, OptimizationContext, PaintProgram, PaintStats, ProgramId,
    execution::{Completion, Input, PaintDemand, Preparation, TaskId},
    materialization::Recipe,
    program::FragmentCall,
    residency::{Owner, RasterId, Residency, Retention},
};
use crate::{
    Rect,
    cost::{self, Work, cache::CacheCost},
};
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

#[derive(Default)]
pub(super) struct Fragments {
    pub selected: HashMap<ProgramId, RasterId>,
    nearby: HashSet<ProgramId>,
    retry: bool,
    waiting_on_coverage: Option<crate::damage::Region>,
    observations: HashMap<ProgramId, Observation>,
    epoch: u64,
}

#[derive(Default)]
struct Observation {
    epoch: u64,
    paints: u32,
    required: u32,
}

struct Demand {
    program: PaintProgram,
    live_uses: usize,
    live_pixels: f64,
    paint_pixels: f64,
    stable: bool,
}

pub(super) fn allowed(context: &OptimizationContext) -> bool {
    context.get::<CacheHint>() != Some(&CacheHint::Dynamic)
}

impl FragmentCall {
    fn placed(&self, origin: (i32, i32), clip: Rect) -> Option<((i32, i32), Rect)> {
        let clip = self.clip.map_or(clip, |c| c.visible(origin, clip));
        let origin = (
            origin.0.checked_add(self.origin.0)?,
            origin.1.checked_add(self.origin.1)?,
        );
        (!self.program.bounds().visible(origin, clip).is_empty()).then_some((origin, clip))
    }
}

impl Fragments {
    pub fn needs_preparation(&self, paint: PaintDemand<'_>) -> bool {
        !paint.is_empty()
            && (self.retry
                || self.waiting_on_coverage.as_ref().is_some_and(
                    |previous| !matches!(paint, PaintDemand::Repair(next) if previous == next),
                ))
    }

    pub fn begin_frame(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
    }

    /// Returns the scheduled work and whether an existing raster binding changed
    /// immediately (warm acquisition or release, neither of which schedules a task).
    pub fn prepare<'a>(
        &mut self,
        roots: impl IntoIterator<Item = (&'a PaintProgram, (i32, i32), Rect)>,
        paint: PaintDemand<'_>,
        residency: &mut Residency,
        plan: &mut Preparation,
        stable_repaints: u32,
        now: Instant,
        stats: &mut PaintStats,
    ) -> (Vec<(ProgramId, TaskId)>, bool) {
        let mut demands = Vec::new();
        let mut indices = HashMap::new();
        for (program, origin, clip) in roots {
            demand(
                program,
                origin,
                clip,
                paint,
                &self.selected,
                &mut demands,
                &mut indices,
            );
        }
        let stable_repaints = stable_repaints.max(2);
        let threads = residency.host.threads();
        self.observations.retain(|id, _| indices.contains_key(id));
        stats.fragment_candidates = demands.len();
        let mut selected = HashMap::new();
        let mut bindings_changed = false;
        let mut tasks = Vec::new();
        let mut substituted = HashSet::new();
        self.nearby.clear();
        self.retry = false;
        let mut missing_coverage = false;
        for demand in demands {
            let program = demand.program;
            let id = program.id();
            if program.analysis().effects.barrier || program.bounds().is_empty() {
                continue;
            }
            if demand.live_uses > 1 {
                self.nearby.insert(id);
            }
            if let Some(raster) = self.selected.remove(&id) {
                plan.bind_fragment(id, Input::Ready(residency.image(raster).clone()));
                selected.insert(id, raster);
                substituted.insert(id);
                continue;
            }
            if demand.paint_pixels == 0.0 {
                missing_coverage = true;
                continue;
            }
            let area = cost::area(program.bounds());
            let cost = CacheCost::new(effective_work(&program, &substituted), area, threads);
            let Some(required_uses) = cost.required_uses() else {
                self.observations.remove(&id);
                continue;
            };
            let recipe = Recipe::item(&program);
            if let Some(raster) = residency.acquire(Owner::Item(id), &recipe) {
                bindings_changed = true;
                plan.bind_fragment(id, Input::Ready(residency.image(raster).clone()));
                selected.insert(id, raster);
                substituted.insert(id);
                continue;
            }
            let uses = demand.paint_pixels / area as f64;
            let observation = self.observations.entry(id).or_default();
            observation.required = ((required_uses / uses).ceil() as u32).max(stable_repaints);
            let observed = if observation.paints >= stable_repaints {
                observation.paints
            } else {
                1
            };
            let horizon = observed.max(if demand.stable { stable_repaints } else { 1 });
            if uses * f64::from(horizon) < required_uses {
                missing_coverage |= demand.paint_pixels < demand.live_pixels;
                continue;
            }
            if let Some(task) = plan.admit(residency, recipe, None, Vec::new(), 1) {
                plan.bind_fragment(id, Input::Task(task));
                tasks.push((id, task));
                substituted.insert(id);
            } else {
                stats.rejected_rasters += 1;
                self.retry = true;
            }
        }
        bindings_changed |= !self.selected.is_empty();
        for (_, raster) in self.selected.drain() {
            residency.release(raster, Retention::Discard, now);
        }
        self.selected = selected;
        self.waiting_on_coverage = match paint {
            PaintDemand::Repair(region) if missing_coverage => Some(region.clone()),
            _ => None,
        };
        (tasks, bindings_changed)
    }

    pub fn replayed(&mut self, program: &PaintProgram) {
        let id = program.id();
        let Some(observation) = self.observations.get_mut(&id) else {
            return;
        };
        if observation.epoch != self.epoch && observation.paints < observation.required {
            observation.epoch = self.epoch;
            observation.paints += 1;
            self.retry |= observation.paints == observation.required;
        }
    }

    pub fn completed(&mut self, id: ProgramId, paints: u32) {
        if let Some(o) = self.observations.get_mut(&id)
            && o.paints < o.required
        {
            o.paints = o.paints.saturating_add(paints).min(o.required);
            self.retry |= o.paints == o.required;
        }
    }

    pub fn publish(
        &mut self,
        tasks: &[(ProgramId, TaskId)],
        plan: &mut Preparation,
        results: &Completion,
        residency: &mut Residency,
        stats: &mut PaintStats,
    ) {
        for &(id, task) in tasks {
            plan.complete(task);
            if let Some(result) = results.result(task) {
                let raster =
                    residency.insert(Owner::Item(id), result.clone(), results.elapsed(task));
                self.selected.insert(id, raster);
                stats.prepared_fragments += usize::from(plan.rasterized(task));
            }
        }
        for id in results.used_fragments() {
            if let Some(&raster) = self.selected.get(id) {
                residency.drawn(raster);
            }
        }
    }

    pub fn retain_for_draw<'a>(
        &mut self,
        roots: impl IntoIterator<Item = (&'a PaintProgram, (i32, i32), Rect)>,
        residency: &mut Residency,
        now: Instant,
    ) {
        if self.selected.is_empty() {
            return;
        }
        let mut used = HashSet::new();
        for (program, origin, clip) in roots {
            used_by(program, origin, clip, &self.selected, &mut used);
        }
        self.selected.retain(|id, raster| {
            if used.contains(id) {
                return true;
            }
            residency.release(
                *raster,
                if self.nearby.contains(id) {
                    Retention::Nearby
                } else {
                    Retention::Discard
                },
                now,
            );
            false
        });
    }
}

fn effective_work(program: &PaintProgram, substituted: &HashSet<ProgramId>) -> Work {
    let mut work = program.cost(true).work(cost::area(program.bounds()));
    for call in program.calls() {
        let bounds = call.program.bounds().translated(call.origin);
        let visible = cost::area(call.clip.map_or(bounds, |clip| clip.intersection(bounds)));
        let child = if allowed(&call.context) && substituted.contains(&call.program.id()) {
            Work::image(cost::area(bounds))
        } else {
            effective_work(&call.program, substituted)
        };
        work.add(child.density(cost::area(bounds)).work(visible));
    }
    work
}

fn demand(
    program: &PaintProgram,
    origin: (i32, i32),
    clip: Rect,
    paint: PaintDemand<'_>,
    selected: &HashMap<ProgramId, RasterId>,
    demands: &mut Vec<Demand>,
    indices: &mut HashMap<ProgramId, usize>,
) {
    for call in program.calls() {
        let Some((origin, clip)) = call.placed(origin, clip) else {
            continue;
        };
        if !allowed(&call.context) || !selected.contains_key(&call.program.id()) {
            demand(
                &call.program,
                origin,
                clip,
                paint,
                selected,
                demands,
                indices,
            );
        }
        if !allowed(&call.context) {
            continue;
        }
        let index = *indices.entry(call.program.id()).or_insert_with(|| {
            let index = demands.len();
            demands.push(Demand {
                program: call.program.clone(),
                live_uses: 0,
                live_pixels: 0.0,
                paint_pixels: 0.0,
                stable: false,
            });
            index
        });
        demands[index].live_uses += 1;
        let visible = call.program.bounds().visible(origin, clip);
        demands[index].live_pixels += f64::from(visible.width) * f64::from(visible.height);
        demands[index].paint_pixels += paint.area(visible) as f64;
        demands[index].stable |= call.context.get::<CacheHint>() == Some(&CacheHint::Stable);
    }
}

fn used_by(
    program: &PaintProgram,
    origin: (i32, i32),
    clip: Rect,
    selected: &HashMap<ProgramId, RasterId>,
    used: &mut HashSet<ProgramId>,
) {
    for call in program.calls() {
        let Some((origin, clip)) = call.placed(origin, clip) else {
            continue;
        };
        if allowed(&call.context) && selected.contains_key(&call.program.id()) {
            used.insert(call.program.id());
        } else {
            used_by(&call.program, origin, clip, selected, used);
        }
    }
}

pub(super) fn program_ids(program: &PaintProgram, ids: &mut HashSet<ProgramId>) {
    if ids.insert(program.id()) {
        for call in program.calls() {
            program_ids(&call.program, ids);
        }
    }
}
