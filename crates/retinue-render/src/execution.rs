use super::{
    CacheHint, PaintProgram, PaintStats, ProgramId, RenderHost, ScopeId,
    materialization::{Recipe, Repair, area},
    residency::{Reservation, Residency},
};
use crate::{Rect, painter::RasterImage};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    ops::Range,
    sync::Arc,
};

mod completion;
pub(crate) mod parallel;
pub(super) use completion::{Completion, Outcome};

#[derive(Clone)]
pub(super) struct Materialization {
    pub image: Arc<RasterImage>,
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
    pub fn area(self, bounds: Rect) -> u64 {
        let area = |rect: Rect| u64::from(rect.width) * u64::from(rect.height);
        match self {
            Self::Full => area(bounds),
            Self::Repair(region) => region
                .rects()
                .iter()
                .map(|rect| area(rect.intersection(bounds)))
                .sum(),
        }
    }

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
    Ready(Arc<RasterImage>),
    Task(TaskId),
}

impl Input {
    fn resolve<'a>(&'a self, results: &'a (impl Inputs + ?Sized)) -> Option<&'a Arc<RasterImage>> {
        match self {
            Self::Ready(image) => Some(image),
            Self::Task(task) => results.image(*task),
        }
    }
}

trait Inputs {
    fn image(&self, task: TaskId) -> Option<&Arc<RasterImage>>;
}

impl Inputs for [Outcome] {
    fn image(&self, task: TaskId) -> Option<&Arc<RasterImage>> {
        self[task.0].result().map(|result| &result.image)
    }
}

impl Inputs for Vec<Outcome> {
    fn image(&self, task: TaskId) -> Option<&Arc<RasterImage>> {
        self.as_slice().image(task)
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
    fragments: HashMap<ProgramId, Input>,
}

struct Rendered {
    image: Arc<RasterImage>,
    reads: HashSet<ProgramId>,
    blits: usize,
    substitutions: usize,
}

#[derive(Default)]
pub(super) struct Preparation {
    tasks: Vec<Arc<Task>>,
    reservations: Vec<Option<Reservation>>,
    pub edges: usize,
    fragments: HashMap<ProgramId, Input>,
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
        let repair = recipe.repair(previous.as_ref().map(|old| &old.recipe));
        let reservation = if repair.regions.is_empty() {
            None
        } else {
            Some(residency.reserve(&recipe)?)
        };
        let task = self.add_planned(recipe, previous, repair, substitutes, threads);
        self.reservations[task.0] = reservation;
        Some(task)
    }

    pub fn add(
        &mut self,
        recipe: Recipe,
        previous: Option<Materialization>,
        substitutes: Vec<Substitute>,
        threads: u32,
    ) -> TaskId {
        let repair = recipe.repair(previous.as_ref().map(|old| &old.recipe));
        self.add_planned(recipe, previous, repair, substitutes, threads)
    }

    fn add_planned(
        &mut self,
        recipe: Recipe,
        previous: Option<Materialization>,
        repair: Repair,
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
        self.tasks.push(Arc::new(Task {
            recipe,
            previous,
            repair,
            substitutes,
            threads,
            fragments,
        }));
        self.reservations.push(None);
        id
    }

    #[cfg(test)]
    pub fn execute(&self, stats: &mut PaintStats) -> Completion {
        self.execute_with(stats, |task, results| task.render(results).ok())
    }

    pub fn execute_on(
        &self,
        host: &RenderHost,
        fonts: &Arc<crate::text::preparation::PreparedFonts>,
    ) -> Completion {
        if self
            .tasks
            .iter()
            .filter(|task| task.pixels() != 0)
            .take(2)
            .count()
            > 1
            && let Some(running) = self.dispatch(host, fonts, || {})
        {
            return running.wait(host);
        }
        self.execute_budgeted(Some(host), |task, results, threads| {
            task.render_with_threads(
                results,
                NonZeroU32::new(threads).expect("admitted native pass includes its caller"),
                fonts,
            )
            .ok()
        })
    }

    #[cfg(test)]
    fn execute_with(
        &self,
        stats: &mut PaintStats,
        mut render: impl FnMut(&Task, &[Outcome]) -> Option<Rendered>,
    ) -> Completion {
        let completed = self.execute_budgeted(None, |task, results, _| render(task, results));
        completed.accumulate(stats);
        completed
    }

    fn execute_budgeted(
        &self,
        host: Option<&RenderHost>,
        mut render: impl FnMut(&Task, &[Outcome], u32) -> Option<Rendered>,
    ) -> Completion {
        let mut results = Completion::new(self.tasks.len());
        results.edges = self.edges;
        for (index, task) in self.tasks.iter().enumerate() {
            let (inputs, output) = results.outputs.split_at_mut(index);
            let pixels = task.pixels();
            let threads = if pixels == 0 {
                Some(task.threads.max(1))
            } else {
                host.map_or(Some(task.threads), |host| host.admit(pixels, task.threads))
            };
            let Some(threads) = threads else {
                output[0] = Outcome::Deferred;
                continue;
            };
            output[0] = task.run_with(inputs, threads, &mut render);
            if pixels != 0
                && let Some(host) = host
            {
                host.charge(std::time::Duration::from_nanos(output[0].elapsed()));
            }
        }
        results
    }

    pub fn rasterized(&self, task: TaskId) -> bool {
        !self.tasks[task.0].repair.regions.is_empty()
    }

    pub fn complete(&mut self, task: TaskId) {
        self.reservations[task.0] = None;
    }

    pub fn reserve(&mut self, task: TaskId, reservation: Reservation) {
        self.reservations[task.0] = Some(reservation);
    }
}

impl Task {
    fn pixels(&self) -> usize {
        self.repair.regions.iter().map(|r| area(*r)).sum::<usize>()
            + if self.repair.copy && !self.repair.regions.is_empty() {
                area(self.recipe.canvas())
            } else {
                0
            }
    }

    fn run_with<R: Inputs + ?Sized>(
        &self,
        inputs: &R,
        threads: u32,
        render: &mut impl FnMut(&Self, &R, u32) -> Option<Rendered>,
    ) -> Outcome {
        let started = std::time::Instant::now();
        let rendered = render(self, inputs, threads);
        let elapsed = started.elapsed().as_nanos() as u64;
        match rendered {
            Some(rendered) => Outcome::Finished {
                result: Some(Materialization {
                    image: rendered.image,
                    recipe: self.recipe.clone(),
                }),
                elapsed,
                reads: rendered.reads,
                blits: rendered.blits,
                substitutions: rendered.substitutions,
                repaired_pixels: self.repair.regions.iter().map(|r| area(*r)).sum(),
                copied_pixels: if self.repair.copy && !self.repair.regions.is_empty() {
                    area(self.recipe.canvas())
                } else {
                    0
                },
            },
            None => Outcome::Finished {
                result: None,
                elapsed,
                reads: HashSet::new(),
                blits: 0,
                substitutions: 0,
                repaired_pixels: 0,
                copied_pixels: 0,
            },
        }
    }

    #[cfg(test)]
    fn render(&self, results: &[Outcome]) -> Result<Rendered, crate::painter::PainterError> {
        let (fonts, _) = crate::text::preparation::PreparedFonts::default().update(
            self.recipe
                .fragments
                .iter()
                .map(|fragment| &fragment.program),
        );
        self.render_with_threads(
            results,
            crate::painter::render_thread_budget(self.threads),
            &Arc::new(fonts),
        )
    }

    fn render_with_threads(
        &self,
        results: &(impl Inputs + ?Sized),
        threads: NonZeroU32,
        fonts: &Arc<crate::text::preparation::PreparedFonts>,
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
            threads,
            |painter| {
                painter.with_text_fonts(fonts, |painter| {
                    if self.repair.copy {
                        let previous = self.previous.as_ref().unwrap();
                        painter.seed_image(&previous.image);
                    }
                    for region in &self.repair.regions {
                        painter.viewport(*region, (0, 0), |painter| {
                            if self.repair.copy {
                                painter.clear();
                            }
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
                })
            },
        )?;
        Ok(Rendered {
            image: Arc::new(image),
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
        results: &(impl Inputs + ?Sized),
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
