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

#[allow(clippy::too_many_arguments)]
pub(super) fn execute<'a>(
    target: Target,
    epoch: u64,
    size: (u32, u32),
    snapshot: Option<Rc<Snapshot>>,
    previous: Option<&Snapshot>,
    source: Option<(Rc<Snapshot>, &'a [u8])>,
    #[cfg(any(test, feature = "benchmark"))] copy_backend: CopyBackend,
    #[cfg(any(test, feature = "benchmark"))] copy_workers: &mut CopyWorkers,
    relocation: Relocation,
    repaint: Repaint,
    pixels: &'a mut [u8],
    threads: NonZeroU32,
    paint: impl FnOnce(&mut Painter<'a>, &Region) -> Result<(), String>,
) -> Result<Submission, String> {
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
    #[cfg(any(test, feature = "benchmark"))]
    let planned = std::time::Instant::now();
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
            if let (Some(plan), Some((_, source))) = (&plan, &source) {
                plan.apply_from(source, pixels).map_err(str::to_owned)?;
            }
            false
        }
        CopyBackend::CpuParallel => {
            if let (Some(plan), Some((_, source))) = (&plan, &source) {
                copy_workers.apply(plan, source, pixels, threads.get())?;
            }
            false
        }
    };
    let source = source
        .filter(|_| native_copy)
        .filter(|_| plan.as_ref().is_some_and(|plan| plan.copied_pixels() > 0))
        .map(|(_, pixels)| PixelSource::new(pixels, size.0, size.1))
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
        #[cfg(any(test, feature = "benchmark"))]
        copied_pixels: plan.as_ref().map_or(0, Plan::copied_pixels),
        #[cfg(any(test, feature = "benchmark"))]
        repaired_pixels: repair.area(),
        #[cfg(any(test, feature = "benchmark"))]
        planning_ns: (planned - started).as_nanos() as u64,
        #[cfg(any(test, feature = "benchmark"))]
        copy_ns: (copied - planned).as_nanos() as u64,
        #[cfg(any(test, feature = "benchmark"))]
        paint_submit_ns: (painted - copied).as_nanos() as u64,
        #[cfg(any(test, feature = "benchmark"))]
        finish_ns: painted.elapsed().as_nanos() as u64,
    })
}
