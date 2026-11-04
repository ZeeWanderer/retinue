use super::{
    CacheHint, PaintProgram, PaintStats, ProgramId, RenderHost, ScopeId,
    materialization::{Recipe, Repair, area},
    residency::{Reservation, Residency},
};
use crate::{
    Rect,
    painter::{RasterImage, render_thread_budget},
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
};

#[derive(Clone)]
pub(super) struct Materialization {
    pub image: Rc<RasterImage>,
    pub recipe: Recipe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct TaskId(usize);

#[derive(Clone, Copy)]
pub(super) enum PaintDemand<'a> {
    Full,
    Repair(&'a crate::damage::Region),
}

impl PaintDemand<'_> {
    pub fn intersects(self, bounds: Rect) -> bool {
        !bounds.is_empty()
            && match self {
                Self::Full => true,
                Self::Repair(region) => region.intersects(bounds),
            }
    }

    pub fn is_empty(self) -> bool {
        matches!(self, Self::Repair(region) if region.is_empty())
    }
}

#[derive(Clone)]
pub(super) enum Input {
    Ready(Rc<RasterImage>),
    Task(TaskId),
}

impl Input {
    pub fn resolve<'a>(
        &'a self,
        results: &'a [Option<Materialization>],
    ) -> Option<&'a Rc<RasterImage>> {
        match self {
            Self::Ready(image) => Some(image),
            Self::Task(task) => results[task.0].as_ref().map(|result| &result.image),
        }
    }
}

pub(super) struct Substitute {
    pub fragments: Range<usize>,
    pub regions: Vec<SourceRegion>,
}

pub(super) struct SourceRegion {
    pub source: Input,
    pub origin: (i32, i32),
    pub clip: Rect,
}

pub(super) struct Task {
    recipe: Recipe,
    previous: Option<Materialization>,
    repair: Repair,
    substitutes: Vec<Substitute>,
    threads: u32,
    reservation: Option<Reservation>,
    fragments: HashMap<ProgramId, Input>,
}

struct Rendered {
    image: Rc<RasterImage>,
    reads: HashSet<ProgramId>,
    blits: usize,
    substitutions: usize,
}

#[derive(Default)]
pub(super) struct Preparation {
    tasks: Vec<Task>,
    pub edges: usize,
    elapsed: Vec<u64>,
    fragments: HashMap<ProgramId, Input>,
    pub used_fragments: HashSet<ProgramId>,
    deferred: HashSet<TaskId>,
}

impl Preparation {
    pub fn bind_fragment(&mut self, program: ProgramId, input: Input) {
        self.fragments.insert(program, input);
    }

    pub fn admit(
        &mut self,
        residency: &mut Residency,
        recipe: Recipe,
        previous: Option<Materialization>,
        substitutes: Vec<Substitute>,
        threads: u32,
    ) -> Option<TaskId> {
        let reservation = if recipe
            .repair(previous.as_ref().map(|old| &old.recipe))
            .regions
            .is_empty()
        {
            None
        } else {
            Some(residency.reserve(&recipe)?)
        };
        let task = self.add(recipe, previous, substitutes, threads);
        self.tasks[task.0].reservation = reservation;
        Some(task)
    }

    pub fn add(
        &mut self,
        recipe: Recipe,
        previous: Option<Materialization>,
        substitutes: Vec<Substitute>,
        threads: u32,
    ) -> TaskId {
        assert!(
            !recipe
                .fragments
                .iter()
                .any(|p| p.program.analysis().effects.barrier && !p.bounds().is_empty())
        );
        let mut end = 0;
        for substitute in &substitutes {
            assert!(
                substitute.fragments.start >= end
                    && substitute.fragments.start < substitute.fragments.end
            );
            assert!(substitute.fragments.end <= recipe.fragments.len());
            end = substitute.fragments.end;
            for (index, region) in substitute.regions.iter().enumerate() {
                assert!(
                    substitute.regions[..index]
                        .iter()
                        .all(|other| other.clip.intersection(region.clip).is_empty()),
                    "substitute regions must be disjoint"
                );
                if let Input::Task(task) = region.source {
                    assert!(
                        task.0 < self.tasks.len(),
                        "materialization inputs must precede consumers"
                    );
                    self.edges += 1;
                }
            }
        }
        let repair = recipe.repair(previous.as_ref().map(|old| &old.recipe));
        let mut fragments = HashMap::new();
        for fragment in recipe.fragments.iter() {
            collect_inputs(&fragment.program, &self.fragments, &mut fragments);
        }
        for input in fragments.values() {
            if let Input::Task(task) = input {
                assert!(
                    task.0 < self.tasks.len(),
                    "fragment producer must precede its consumer"
                );
                self.edges += 1;
            }
        }
        let id = TaskId(self.tasks.len());
        self.tasks.push(Task {
            recipe,
            previous,
            repair,
            substitutes,
            threads,
            reservation: None,
            fragments,
        });
        id
    }

    #[cfg(test)]
    pub fn execute(&mut self, stats: &mut PaintStats) -> Vec<Option<Materialization>> {
        self.execute_with(stats, |task, results| task.render(results).ok())
    }

    pub fn execute_on(
        &mut self,
        host: &RenderHost,
        stats: &mut PaintStats,
    ) -> Vec<Option<Materialization>> {
        self.execute_budgeted(Some(host), stats, |task, results, threads| {
            task.render_with_threads(results, threads).ok()
        })
    }

    #[cfg(test)]
    fn execute_with(
        &mut self,
        stats: &mut PaintStats,
        mut render: impl FnMut(&Task, &[Option<Materialization>]) -> Option<Rendered>,
    ) -> Vec<Option<Materialization>> {
        self.execute_budgeted(None, stats, |task, results, _| render(task, results))
    }

    fn execute_budgeted(
        &mut self,
        host: Option<&RenderHost>,
        stats: &mut PaintStats,
        mut render: impl FnMut(&Task, &[Option<Materialization>], u32) -> Option<Rendered>,
    ) -> Vec<Option<Materialization>> {
        stats.preparation_tasks += self.tasks.len();
        stats.preparation_edges += self.edges;
        let mut results = Vec::with_capacity(self.tasks.len());
        self.elapsed.clear();
        self.used_fragments.clear();
        self.deferred.clear();
        for (index, task) in self.tasks.iter().enumerate() {
            let pixels = task.repair.regions.iter().map(|r| area(*r)).sum::<usize>()
                + if task.repair.copy && !task.repair.regions.is_empty() {
                    area(task.recipe.canvas())
                } else {
                    0
                };
            let threads = if pixels == 0 {
                Some(task.threads)
            } else {
                host.map_or(Some(task.threads), |host| host.admit(pixels, task.threads))
            };
            let Some(threads) = threads else {
                self.deferred.insert(TaskId(index));
                stats.deferred_preparations += 1;
                self.elapsed.push(0);
                results.push(None);
                continue;
            };
            let started = std::time::Instant::now();
            let result = render(task, &results, threads).map(|rendered| {
                self.used_fragments.extend(rendered.reads);
                stats.fragment_blits += rendered.blits;
                stats.preparation_blits += rendered.substitutions;
                stats.repaired_pixels +=
                    task.repair.regions.iter().map(|r| area(*r)).sum::<usize>();
                if task.repair.copy && !task.repair.regions.is_empty() {
                    stats.copied_pixels += area(task.recipe.canvas());
                }
                Materialization {
                    image: rendered.image,
                    recipe: task.recipe.clone(),
                }
            });
            if pixels != 0
                && let Some(host) = host
            {
                host.charge(started.elapsed());
            }
            stats.failed_preparations += usize::from(result.is_none());
            self.elapsed.push(started.elapsed().as_nanos() as u64);
            results.push(result);
        }
        #[cfg(any(test, feature = "benchmark"))]
        {
            stats.materialization_ns += self.elapsed.iter().sum::<u64>();
        }
        results
    }

    pub fn deferred(&self, task: TaskId) -> bool {
        self.deferred.contains(&task)
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }

    pub fn result<'a>(
        &self,
        task: TaskId,
        results: &'a [Option<Materialization>],
    ) -> Option<&'a Materialization> {
        results[task.0].as_ref()
    }

    pub fn rasterized(&self, task: TaskId) -> bool {
        !self.tasks[task.0].repair.regions.is_empty()
    }

    pub fn elapsed(&self, task: TaskId) -> u64 {
        self.elapsed[task.0]
    }

    pub fn complete(&mut self, task: TaskId) {
        self.tasks[task.0].reservation = None;
    }

    pub fn reserve(&mut self, task: TaskId, reservation: Reservation) {
        self.tasks[task.0].reservation = Some(reservation);
    }
}

impl Task {
    #[cfg(test)]
    fn render(
        &self,
        results: &[Option<Materialization>],
    ) -> Result<Rendered, crate::painter::PainterError> {
        self.render_with_threads(results, self.threads)
    }

    fn render_with_threads(
        &self,
        results: &[Option<Materialization>],
        threads: u32,
    ) -> Result<Rendered, crate::painter::PainterError> {
        if self.repair.regions.is_empty() {
            return Ok(Rendered {
                image: self.previous.as_ref().unwrap().image.clone(),
                reads: HashSet::new(),
                blits: 0,
                substitutions: 0,
            });
        }
        let mut reads = HashSet::new();
        let mut stats = PaintStats::default();
        let mut substitutions = 0;
        let image = RasterImage::render_with_threads(
            self.recipe.size.0,
            self.recipe.size.1,
            render_thread_budget(threads),
            |painter| {
                if self.repair.copy {
                    let previous = self.previous.as_ref().unwrap();
                    painter.draw_image(
                        &previous.image,
                        0,
                        0,
                        self.recipe.size.0,
                        self.recipe.size.1,
                    );
                }
                for region in &self.repair.regions {
                    painter.viewport(*region, (0, 0), |painter| {
                        painter.clear();
                        let mut index = 0;
                        let mut substitutes = self.substitutes.iter().peekable();
                        while index < self.recipe.fragments.len() {
                            if let Some(substitute) =
                                substitutes.peek().filter(|s| s.fragments.start == index)
                            {
                                let mut covered = Vec::new();
                                for source in &substitute.regions {
                                    let Some(image) = source.source.resolve(results) else {
                                        continue;
                                    };
                                    let (width, height) = image.dimensions();
                                    let clip = super::PaintExtent {
                                        x: 0,
                                        y: 0,
                                        width,
                                        height,
                                    }
                                    .visible(source.origin, source.clip)
                                    .intersection(*region);
                                    if clip.is_empty() {
                                        continue;
                                    }
                                    painter.viewport(clip, (0, 0), |painter| {
                                        painter.translated(source.origin, |painter| {
                                            let (width, height) = image.dimensions();
                                            painter.draw_image(image, 0, 0, width, height);
                                        });
                                    });
                                    covered.push(clip);
                                    substitutions += 1;
                                }
                                for gap in super::coverage::remaining(*region, covered) {
                                    painter.viewport(gap, (0, 0), |painter| {
                                        self.replay(
                                            substitute.fragments.clone(),
                                            painter,
                                            gap,
                                            results,
                                            &mut reads,
                                            &mut stats,
                                        );
                                    });
                                }
                                index = substitute.fragments.end;
                                substitutes.next();
                                continue;
                            }
                            self.replay(
                                index..index + 1,
                                painter,
                                *region,
                                results,
                                &mut reads,
                                &mut stats,
                            );
                            index += 1;
                        }
                    });
                }
            },
        )?;
        Ok(Rendered {
            image: Rc::new(image),
            reads,
            blits: stats.fragment_blits,
            substitutions,
        })
    }

    fn replay(
        &self,
        range: Range<usize>,
        painter: &mut crate::painter::Painter<'_>,
        repair: Rect,
        results: &[Option<Materialization>],
        reads: &mut HashSet<ProgramId>,
        stats: &mut PaintStats,
    ) {
        for index in range {
            self.recipe.paint_fragment_with(
                index,
                painter,
                repair,
                &mut |program, context| {
                    if context.get::<CacheHint>() == Some(&CacheHint::Dynamic) {
                        return None;
                    }
                    let image = self.fragments.get(&program.id())?.resolve(results)?.clone();
                    reads.insert(program.id());
                    Some(image)
                },
                stats,
            );
        }
    }
}

fn collect_inputs(
    program: &PaintProgram,
    available: &HashMap<ProgramId, Input>,
    output: &mut HashMap<ProgramId, Input>,
) {
    for call in program.calls() {
        if let Some(input) = available.get(&call.program.id()) {
            output.insert(call.program.id(), input.clone());
        }
        collect_inputs(&call.program, available, output);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrawStep {
    Replay {
        instance: usize,
    },
    Item {
        instance: usize,
    },
    Clipped {
        instance: usize,
        regions: Vec<Rect>,
        raster: bool,
    },
    Layer {
        group: ScopeId,
        instances: Range<usize>,
    },
}

#[cfg(test)]
mod tests;
