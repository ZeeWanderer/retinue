use super::{
    CacheHint, OptimizationContext, PaintProgram, PaintStats, ProgramId,
    execution::{Input, Materialization, Preparation, TaskId},
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
}

struct Demand {
    program: PaintProgram,
    uses: usize,
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
        residency: &mut Residency,
        plan: &mut Preparation,
        min_work: usize,
        now: Instant,
        stats: &mut PaintStats,
    ) -> Vec<(ProgramId, TaskId)> {
        let mut demands = Vec::new();
        let mut indices = HashMap::new();
        for (program, origin, clip) in roots {
            demand(program, origin, clip, &mut demands, &mut indices);
        }
        stats.fragment_candidates = demands.len();
        let mut selected = HashMap::new();
        let mut tasks = Vec::new();
        self.nearby.clear();
        for demand in demands {
            let program = demand.program;
            let id = program.id();
            if program.analysis().effects.barrier || program.bounds().is_empty() {
                continue;
            }
            if demand.uses > 1 {
                self.nearby.insert(id);
            }
            if let Some(raster) = self.selected.remove(&id) {
                plan.bind_fragment(id, Input::Ready(residency.image(raster).clone()));
                selected.insert(id, raster);
                continue;
            }
            let work = program
                .summary()
                .work
                .saturating_sub(1)
                .saturating_mul(demand.uses);
            if work < min_work || (demand.uses < 2 && !demand.stable) {
                continue;
            }
            let recipe = Recipe::item(&program);
            if let Some(raster) = residency.acquire(Owner::Item(id), &recipe) {
                plan.bind_fragment(id, Input::Ready(residency.image(raster).clone()));
                selected.insert(id, raster);
            } else if let Some(task) = plan.admit(residency, recipe, None, Vec::new(), 1) {
                plan.bind_fragment(id, Input::Task(task));
                tasks.push((id, task));
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

fn demand(
    program: &PaintProgram,
    origin: (i32, i32),
    clip: Rect,
    demands: &mut Vec<Demand>,
    indices: &mut HashMap<ProgramId, usize>,
) {
    for call in program.calls() {
        let Some((origin, clip)) = call.placed(origin, clip) else {
            continue;
        };
        demand(&call.program, origin, clip, demands, indices);
        if !allowed(&call.context) {
            continue;
        }
        let index = *indices.entry(call.program.id()).or_insert_with(|| {
            let index = demands.len();
            demands.push(Demand {
                program: call.program.clone(),
                uses: 0,
                stable: false,
            });
            index
        });
        demands[index].uses += 1;
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
