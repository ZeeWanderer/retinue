use super::*;
use crate::{damage::Repaint, painter::PixelSource};

#[cfg(any(test, feature = "benchmark"))]
#[derive(Default)]
pub(super) struct CopyWorkers(Option<(u32, rayon::ThreadPool)>);

#[cfg(any(test, feature = "benchmark"))]
impl CopyWorkers {
    fn apply(
        &mut self,
        plan: &Plan,
        source: &[u8],
        target: &mut [u8],
        threads: u32,
    ) -> Result<(), String> {
        if threads <= 1 || plan.copied_pixels() == 0 {
            return plan.apply_from(source, target).map_err(str::to_owned);
        }
        if self.0.as_ref().is_none_or(|(n, _)| *n != threads) {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads as usize - 1)
                .thread_name(|n| format!("relocate-{n}"))
                .build()
                .map_err(|e| e.to_string())?;
            self.0 = Some((threads, pool));
        }
        plan.apply_parallel(&self.0.as_ref().unwrap().1, source, target, threads)
            .map_err(str::to_owned)
    }
}

/// History-bound repair work for an acquired target. Prepare dependencies before executing it.
pub struct Frame<'a> {
    work: Work,
    pixels: &'a mut [u8],
    source: Option<&'a [u8]>,
}

/// Owned repair instructions. The target and donor remain exclusive host buffer leases.
pub struct Work {
    target: Target,
    epoch: u64,
    size: (u32, u32),
    snapshot: Option<Arc<Snapshot>>,
    plan: Option<Plan>,
    repaint: Repaint,
    source: bool,
    #[cfg(any(test, feature = "benchmark"))]
    planning_ns: u64,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan<'a>(
    target: Target,
    epoch: u64,
    size: (u32, u32),
    snapshot: Option<Arc<Snapshot>>,
    previous: Option<&Snapshot>,
    source: Option<(Arc<Snapshot>, &'a [u8])>,
    relocation: Relocation,
    repaint: Repaint,
    pixels: &'a mut [u8],
    threads: NonZeroU32,
) -> Result<Frame<'a>, String> {
    #[cfg(any(test, feature = "benchmark"))]
    let started = std::time::Instant::now();
    if let Some((scene, pixels)) = &source {
        validate(size, pixels, Some(scene))?;
    }
    let plan = snapshot
        .as_ref()
        .filter(|_| relocation != Relocation::Disabled)
        .map(|scene| {
            Plan::with_execution(
                scene,
                source
                    .as_ref()
                    .map(|(scene, _)| scene.as_ref())
                    .or(previous),
                &repaint.repair,
                crate::cost::Execution {
                    threads: threads.get(),
                    copy: if source.is_some() {
                        crate::cost::CopyExecution::Queued
                    } else {
                        crate::cost::CopyExecution::InPlace
                    },
                },
            )
        });
    Ok(Frame {
        work: Work {
            target,
            epoch,
            size,
            snapshot,
            plan,
            repaint,
            source: source.is_some(),
            #[cfg(any(test, feature = "benchmark"))]
            planning_ns: started.elapsed().as_nanos() as u64,
        },
        pixels,
        source: source.map(|(_, pixels)| pixels),
    })
}

impl Work {
    pub fn repair(&self) -> &Region {
        self.plan
            .as_ref()
            .map_or(&self.repaint.repair, |plan| &plan.repair)
    }

    pub fn execute<'a>(
        self,
        pixels: &'a mut [u8],
        source: Option<&'a [u8]>,
        threads: NonZeroU32,
        paint: impl FnOnce(&mut Painter<'a>, &Region) -> Result<(), String>,
    ) -> Result<Submission, String> {
        validate(self.size, pixels, self.snapshot.as_deref())?;
        let source = if self.source {
            let source = source.ok_or("repair donor lease is missing")?;
            validate(self.size, source, None)?;
            Some(source)
        } else {
            None
        };
        Frame {
            work: self,
            pixels,
            source,
        }
        .execute(threads, paint)
    }
}

impl<'a> Frame<'a> {
    pub fn repair(&self) -> &Region {
        self.work.repair()
    }

    pub fn into_work(self) -> Work {
        self.work
    }

    pub fn execute(
        self,
        threads: NonZeroU32,
        paint: impl FnOnce(&mut Painter<'a>, &Region) -> Result<(), String>,
    ) -> Result<Submission, String> {
        self.execute_with(
            threads,
            #[cfg(any(test, feature = "benchmark"))]
            CopyBackend::Queued,
            #[cfg(any(test, feature = "benchmark"))]
            &mut CopyWorkers::default(),
            paint,
        )
    }

    pub(super) fn execute_with(
        self,
        threads: NonZeroU32,
        #[cfg(any(test, feature = "benchmark"))] copy_backend: CopyBackend,
        #[cfg(any(test, feature = "benchmark"))] copy_workers: &mut CopyWorkers,
        paint: impl FnOnce(&mut Painter<'a>, &Region) -> Result<(), String>,
    ) -> Result<Submission, String> {
        #[cfg(any(test, feature = "benchmark"))]
        let started = std::time::Instant::now();
        let Self {
            work:
                Work {
                    target,
                    epoch,
                    size,
                    snapshot,
                    plan,
                    repaint,
                    source: _,
                    #[cfg(any(test, feature = "benchmark"))]
                    planning_ns,
                },
            pixels,
            source,
        } = self;
        let repair = plan.as_ref().map_or(&repaint.repair, |plan| &plan.repair);
        if source.is_none()
            && let Some(plan) = &plan
        {
            plan.apply(pixels).map_err(str::to_owned)?;
        }
        #[cfg(not(any(test, feature = "benchmark")))]
        let native_copy = true;
        #[cfg(any(test, feature = "benchmark"))]
        let native_copy = match copy_backend {
            CopyBackend::Queued => true,
            CopyBackend::Cpu => {
                if let (Some(plan), Some(source)) = (&plan, &source) {
                    plan.apply_from(source, pixels).map_err(str::to_owned)?;
                }
                false
            }
            CopyBackend::CpuParallel => {
                if let (Some(plan), Some(source)) = (&plan, &source) {
                    copy_workers.apply(plan, source, pixels, threads.get())?;
                }
                false
            }
        };
        let source = source
            .filter(|_| native_copy)
            .filter(|_| plan.as_ref().is_some_and(|plan| plan.copied_pixels() > 0))
            .map(|pixels| PixelSource::new(pixels, size.0, size.1))
            .transpose()
            .map_err(|e| e.to_string())?;
        let mut painter =
            Painter::with_threads(pixels, size.0, size.1, threads).map_err(|e| e.to_string())?;
        if let Some(source) = &source {
            plan.as_ref().unwrap().enqueue(source, &mut painter);
        }
        #[cfg(any(test, feature = "benchmark"))]
        let copied = std::time::Instant::now();
        painter.repair(repair);
        paint(&mut painter, repair)?;
        #[cfg(any(test, feature = "benchmark"))]
        let painted = std::time::Instant::now();
        painter.finish().map_err(|e| e.to_string())?;
        Ok(Submission {
            target,
            epoch,
            snapshot,
            damage: repaint.damage,
            ticket: repaint.ticket,
            #[cfg(any(test, feature = "benchmark"))]
            copied_pixels: plan.as_ref().map_or(0, Plan::copied_pixels),
            #[cfg(any(test, feature = "benchmark"))]
            repaired_pixels: repair.area(),
            #[cfg(any(test, feature = "benchmark"))]
            planning_ns,
            #[cfg(any(test, feature = "benchmark"))]
            copy_ns: (copied - started).as_nanos() as u64,
            #[cfg(any(test, feature = "benchmark"))]
            paint_submit_ns: (painted - copied).as_nanos() as u64,
            #[cfg(any(test, feature = "benchmark"))]
            finish_ns: painted.elapsed().as_nanos() as u64,
        })
    }
}
