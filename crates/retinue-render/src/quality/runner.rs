use super::{
    Cache, Policy, Repair,
    scenarios::{Assets, Case, Fixture, Step},
};
use crate::{
    Color, DrawStep, LayerPolicy, PaintStats, RasterMode, Renderer, ResidencyPolicy,
    damage::Damage,
    surface::{
        Snapshot,
        experiment::{Transfer, Trial},
    },
};
use rustix::time::{ClockId, clock_gettime};
use serde::Serialize;
use serde_json::{Value, json};
use std::{num::NonZeroU32, time::Instant};

const BACKGROUND: Color = Color::rgb(0x18202a);

#[derive(Default, Clone, Serialize)]
pub(super) struct FrameSample {
    pub total_ns: u64,
    pub cpu_ns: u64,
    pub update_ns: u64,
    pub snapshot_ns: u64,
    pub prepare_ns: u64,
    pub plan_ns: u64,
    pub submit_ns: u64,
    pub finish_ns: u64,
    pub copied_pixels: u64,
    pub repaired_pixels: u64,
    pub raster_bytes: usize,
    pub preparation_tasks: usize,
    pub promoted_layers: usize,
    pub demoted_layers: usize,
    pub deferred_tasks: usize,
    pub replayed_commands: usize,
    pub fragment_blits: usize,
    pub actual_threads: u32,
}

#[derive(Default, Serialize)]
pub(super) struct Trace {
    pub setup_ns: u64,
    pub setup_cpu_ns: u64,
    pub total_ns: u64,
    pub cpu_ns: u64,
    pub peak_raster_bytes: usize,
    pub surface_bytes: usize,
    pub frames: Vec<FrameSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Value>,
}

fn cpu() -> u64 {
    let time = clock_gettime(ClockId::ProcessCPUTime);
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}

pub(super) fn run(
    assets: &Assets,
    case: Case,
    steps: &[Step],
    policy: Policy,
    threads: u32,
    diagnostics: bool,
    mut inspect: impl FnMut(usize, &[u8]),
) -> Trace {
    let started = Instant::now();
    let cpu_started = cpu();
    let mut fixture = Fixture::new(case, assets);
    let mut renderer = Renderer::with_layers(
        ResidencyPolicy {
            peak_bytes: case.peak_bytes,
            ..ResidencyPolicy::desktop(0)
        },
        LayerPolicy::desktop(),
    );
    renderer.set_optimizations(policy.compiled);
    renderer.set_materializations(policy.cache != Cache::None);
    let mode = if policy.cache == Cache::Preferred {
        RasterMode::Explicit
    } else {
        RasterMode::Replay
    };
    let transfer = match policy.repair {
        Repair::Full => Transfer::Repair,
        Repair::Damage => Transfer::NoRelocate,
        Repair::Relocate => Transfer::Previous,
    };
    let mut trial = Trial::new(case.size, case.buffers, transfer);
    let mut trace = Trace {
        setup_ns: started.elapsed().as_nanos() as u64,
        setup_cpu_ns: cpu().saturating_sub(cpu_started),
        surface_bytes: trial.bytes(),
        frames: Vec::with_capacity(steps.len()),
        ..Trace::default()
    };
    let mut presented = 0;
    for (index, &step) in steps.iter().enumerate() {
        let cpu_started = cpu();
        let started = Instant::now();
        fixture.update(step);
        let updated = Instant::now();
        let frame = &fixture.frame;
        let host = renderer.host().clone();
        let pass = host.begin_frame(NonZeroU32::new(threads).unwrap());
        let mut snapshot = Snapshot::new(case.size, BACKGROUND);
        snapshot.record_rendered(frame, &renderer, mode, |_, _| false);
        let snapshotted = Instant::now();
        if policy.repair == Repair::Full {
            trial.invalidate(Damage::Full);
        }
        renderer.stats = PaintStats::default();
        let mut prepare_ns = 0;
        let sample = trial
            .paint(
                index % case.buffers,
                snapshot,
                pass.threads(),
                |painter, repair| {
                    let prepared = Instant::now();
                    renderer.prepare_for(frame, mode, repair);
                    prepare_ns += prepared.elapsed().as_nanos() as u64;
                    renderer.paint(frame, painter, case.bounds(), BACKGROUND);
                    Ok(())
                },
            )
            .unwrap();
        let actual_threads = pass.threads().get();
        drop(pass);
        let total_ns = started.elapsed().as_nanos() as u64;
        let cpu_ns = cpu().saturating_sub(cpu_started);
        if sample.is_some() {
            presented = index % case.buffers;
        }
        let sample = sample.unwrap_or_default();
        let stats = renderer.stats;
        let raster_bytes = renderer.residency_stats().live_bytes;
        trace.peak_raster_bytes = trace.peak_raster_bytes.max(raster_bytes);
        trace.frames.push(FrameSample {
            total_ns,
            cpu_ns,
            update_ns: updated.duration_since(started).as_nanos() as u64,
            snapshot_ns: snapshotted.duration_since(updated).as_nanos() as u64,
            prepare_ns,
            plan_ns: sample.planning_ns,
            submit_ns: sample.copy_submit_ns + sample.paint_submit_ns.saturating_sub(prepare_ns),
            finish_ns: sample.finish_ns,
            copied_pixels: sample.copied_pixels,
            repaired_pixels: sample.repaired_pixels,
            raster_bytes,
            preparation_tasks: stats.preparation_tasks,
            promoted_layers: stats.promoted_layers,
            demoted_layers: stats.demoted_layers,
            deferred_tasks: stats.deferred_preparations,
            replayed_commands: stats.replayed_commands,
            fragment_blits: stats.fragment_blits,
            actual_threads,
        });
        if diagnostics {
            let plan: Vec<_> = renderer.execution_plan().map(|step| match step {
                DrawStep::Replay { instance } => json!({"operation": "replay", "instance": instance}),
                DrawStep::Item { instance } => json!({"operation": "raster", "instance": instance}),
                DrawStep::Clipped { instance, regions, raster } => json!({"operation": "clipped", "instance": instance, "raster": raster, "regions": regions.iter().map(|r| [r.x, r.y, r.width, r.height]).collect::<Vec<_>>()}),
                DrawStep::Layer { instances, .. } => json!({"operation": "layer", "instances": [instances.start, instances.end]}),
            }).collect();
            let mut effective = Snapshot::new(case.size, BACKGROUND);
            effective.record_rendered(frame, &renderer, mode, |_, _| false);
            let work = effective.repair_work(&case.bounds().into());
            let removed: usize = frame
                .instances()
                .iter()
                .map(|i| i.program.summary().removed)
                .sum();
            trace.decisions.push(json!({
                "frame": index, "input": step, "plan": plan,
                "remarks": [
                    {"pass": "command-simplification", "kind": if policy.compiled && removed > 0 { "passed" } else { "analysis" }, "source_instances": frame.instances().len(), "removed_commands": if policy.compiled { removed } else { 0 }},
                    {"pass": "materialization", "kind": if stats.preparation_tasks > 0 { "passed" } else { "analysis" }, "prepared": stats.preparation_tasks, "rejected": stats.rejected_rasters, "deferred": stats.deferred_preparations, "resident_bytes": raster_bytes},
                    {"pass": "relocation", "kind": if sample.copied_pixels > 0 { "passed" } else { "analysis" }, "copied_pixels": sample.copied_pixels, "repaired_pixels": sample.repaired_pixels}
                ],
                "effective_full_replay_work": {"solid": work.solid, "blended": work.blended, "sampled": work.sampled, "commands": work.commands, "edges": work.edges, "glyphs": work.glyphs}
            }));
        }
        inspect(index, trial.pixels(presented));
    }
    trace.total_ns = trace.setup_ns + trace.frames.iter().map(|f| f.total_ns).sum::<u64>();
    trace.cpu_ns = trace.setup_cpu_ns + trace.frames.iter().map(|f| f.cpu_ns).sum::<u64>();
    trace
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Mismatch {
    pub frame: usize,
    pub pixel: usize,
    pub channel: usize,
    pub expected: u8,
    pub actual: u8,
}

pub(super) fn reference(assets: &Assets, case: Case, steps: &[Step]) -> Vec<Vec<u8>> {
    let mut pixels = Vec::new();
    run(
        assets,
        case,
        steps,
        Policy::REFERENCE,
        1,
        false,
        |_, image| pixels.push(image.to_vec()),
    );
    pixels
}

pub(super) fn check(
    assets: &Assets,
    case: Case,
    steps: &[Step],
    policy: Policy,
    threads: u32,
    reference: &[Vec<u8>],
) -> Result<Trace, Mismatch> {
    let mut mismatch = None;
    let tolerance = tolerance(case, policy);
    let trace = run(
        assets,
        case,
        steps,
        policy,
        threads,
        true,
        |frame, pixels| {
            if mismatch.is_some() {
                return;
            }
            assert_eq!(pixels.len(), reference[frame].len());
            if let Some((byte, (&actual, &expected))) = pixels
                .iter()
                .zip(&reference[frame])
                .enumerate()
                .find(|(byte, (a, b))| a.abs_diff(**b) > if byte % 4 == 3 { 0 } else { tolerance })
            {
                mismatch = Some(Mismatch {
                    frame,
                    pixel: byte / 4,
                    channel: byte % 4,
                    expected,
                    actual,
                });
            }
        },
    );
    mismatch.map_or(Ok(trace), Err)
}

pub(super) fn tolerance(case: Case, policy: Policy) -> u8 {
    if case.art == super::scenarios::Art::Solid {
        0
    } else if policy.cache == Cache::None {
        // Native curve clipping rounds by one channel unit even without raster caches.
        1
    } else {
        3
    }
}
