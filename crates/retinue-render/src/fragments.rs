use super::{
    CacheHint, OptimizationContext, PaintProgram, PaintStats, ProgramId,
    execution::{Input, Materialization, PaintDemand, Preparation, TaskId},
    materialization::Recipe,
    program::FragmentCall,
    residency::{Owner, RasterId, Residency, Retention},
};
use crate::Rect;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

#[derive(Default)]
pub(super) struct Fragments {
    pub selected: HashMap<ProgramId, RasterId>,
    nearby: HashSet<ProgramId>,
    pub awaiting_demand: bool,
    observations: HashMap<ProgramId, Observation>,
    epoch: u64,
    stable_repaints: u32,
}

#[derive(Default)]
struct Observation {
    epoch: u64,
    paints: u32,
}

struct Demand {
    program: PaintProgram,
    live_uses: usize,
    paint_uses: usize,
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
    pub fn prepare<'a>(
        &mut self,
        roots: impl IntoIterator<Item = (&'a PaintProgram, (i32, i32), Rect)>,
        paint: PaintDemand<'_>,
        residency: &mut Residency,
        plan: &mut Preparation,
        min_work: usize,
        stable_repaints: u32,
        now: Instant,
        stats: &mut PaintStats,
    ) -> Vec<(ProgramId, TaskId)> {
        let mut demands = Vec::new();
        let mut indices = HashMap::new();
        for (program, origin, clip) in roots {
            demand(program, origin, clip, paint, &mut demands, &mut indices);
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.stable_repaints = stable_repaints.max(2);
        self.observations.retain(|id, _| indices.contains_key(id));
        stats.fragment_candidates = demands.len();
        let mut selected = HashMap::new();
        let mut tasks = Vec::new();
        let mut substituted = HashSet::new();
        self.nearby.clear();
        self.awaiting_demand = false;
        for demand in demands {
            let program = demand.program;
            let id = program.id();
            if program.analysis().effects.barrier || program.bounds().is_empty() {
                continue;
            }
            if demand.live_uses > 1 {
                self.nearby.insert(id);
            }
            let mature = program.discovered()
                && self
                    .observations
                    .get(&id)
                    .is_some_and(|o| o.paints >= self.stable_repaints);
            let work = effective_work(&program, &substituted).saturating_sub(1);
            let reuse = if mature {
                self.stable_repaints as usize
            } else {
                1
            };
            if work != 0
                && let Some(raster) = self.selected.remove(&id)
            {
                plan.bind_fragment(id, Input::Ready(residency.image(raster).clone()));
                selected.insert(id, raster);
                substituted.insert(id);
                continue;
            }
            if work.saturating_mul(demand.live_uses.max(reuse)) < min_work
                || (demand.live_uses < 2 && !demand.stable && !mature)
            {
                continue;
            }
            if demand.paint_uses == 0
                || work.saturating_mul(demand.paint_uses.max(reuse)) < min_work
                || (demand.paint_uses < 2 && !demand.stable && !mature)
            {
                self.awaiting_demand = true;
                continue;
            }
            let recipe = Recipe::item(&program);
            if let Some(raster) = residency.acquire(Owner::Item(id), &recipe) {
                plan.bind_fragment(id, Input::Ready(residency.image(raster).clone()));
                selected.insert(id, raster);
                substituted.insert(id);
            } else if let Some(task) = plan.admit(residency, recipe, None, Vec::new(), 1) {
                plan.bind_fragment(id, Input::Task(task));
                tasks.push((id, task));
                substituted.insert(id);
            } else {
                stats.rejected_rasters += 1;
            }
        }
        for (_, raster) in self.selected.drain() {
            residency.release(raster, Retention::Discard, now);
        }
        self.selected = selected;
        tasks
    }

    pub fn replayed(&mut self, program: &PaintProgram) {
        if !program.discovered() || self.stable_repaints == 0 {
            return;
        }
        let observation = self.observations.entry(program.id()).or_default();
        if observation.epoch != self.epoch && observation.paints < self.stable_repaints {
            observation.epoch = self.epoch;
            observation.paints += 1;
            self.awaiting_demand = true;
        }
    }

    pub fn publish(
        &mut self,
        tasks: &[(ProgramId, TaskId)],
        plan: &mut Preparation,
        results: &[Option<Materialization>],
        residency: &mut Residency,
        stats: &mut PaintStats,
    ) {
        for &(id, task) in tasks {
            plan.complete(task);
            if let Some(result) = plan.result(task, results) {
                let raster = residency.insert(Owner::Item(id), result.clone(), plan.elapsed(task));
                self.selected.insert(id, raster);
                stats.prepared_fragments += usize::from(plan.rasterized(task));
            }
        }
        for id in &plan.used_fragments {
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

fn effective_work(program: &PaintProgram, substituted: &HashSet<ProgramId>) -> usize {
    program
        .calls()
        .iter()
        .fold(program.summary().direct_work, |work, call| {
            work.saturating_add(
                if allowed(&call.context) && substituted.contains(&call.program.id()) {
                    1
                } else {
                    effective_work(&call.program, substituted)
                },
            )
        })
}

fn demand(
    program: &PaintProgram,
    origin: (i32, i32),
    clip: Rect,
    paint: PaintDemand<'_>,
    demands: &mut Vec<Demand>,
    indices: &mut HashMap<ProgramId, usize>,
) {
    for call in program.calls() {
        let Some((origin, clip)) = call.placed(origin, clip) else {
            continue;
        };
        demand(&call.program, origin, clip, paint, demands, indices);
        if !allowed(&call.context) {
            continue;
        }
        let index = *indices.entry(call.program.id()).or_insert_with(|| {
            let index = demands.len();
            demands.push(Demand {
                program: call.program.clone(),
                live_uses: 0,
                paint_uses: 0,
                stable: false,
            });
            index
        });
        demands[index].live_uses += 1;
        demands[index].paint_uses +=
            usize::from(paint.intersects(call.program.bounds().visible(origin, clip)));
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
