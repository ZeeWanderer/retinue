use super::*;
use retinue_render::relocate::Target;
use rustix::time::{ClockId, clock_gettime};

mod materialized;

fn run(
    target: &mut Target,
    sample: &TraceFrame,
    renderer: &mut Renderer,
    policy: Policy,
    reuse: bool,
) -> retinue_render::relocate::Sample {
    target
        .paint(
            sample.snapshot.frame(),
            sample.motion,
            Color::rgb(0x121419),
            renderer,
            policy.mode(),
            crate::app::benchmark::threads(),
            reuse,
        )
        .unwrap()
}

fn validate_reuse(trace: &[TraceFrame], policy: Policy, buffers: usize) -> (u8, usize) {
    let mut targets: Vec<_> = (0..buffers).map(|_| Target::default()).collect();
    let size = trace[0].snapshot.frame().size();
    let mut expected = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut renderer = policy.renderer();
    let mut oracle = policy.renderer();
    let mut error = 0;
    let mut reused = 0;
    let mut reasons = std::collections::BTreeMap::new();
    for (index, sample) in trace.iter().enumerate() {
        let target = &mut targets[index % buffers];
        let result = run(target, sample, &mut renderer, policy, true);
        *reasons.entry(result.reason).or_insert(0usize) += 1;
        reused += usize::from(result.copied_pixels > 0);
        render(sample.snapshot.frame(), &mut oracle, policy, &mut expected);
        let glows = glow_regions(sample.snapshot.frame());
        for (pixel, (a, b)) in target.pixels().iter().zip(&expected).enumerate() {
            let diff = a.abs_diff(*b);
            error = error.max(diff);
            if diff <= 1 {
                continue;
            }
            let x = (pixel / 4 % size.0 as usize) as u32;
            let y = (pixel / 4 / size.0 as usize) as u32;
            let glow = glows.iter().any(|r| r.contains(x as f64, y as f64));
            assert!(
                diff <= 1 || (glow && diff <= 3),
                "{policy:?}, {buffers} buffers, frame {index}: fresh pixel {x},{y} channel {} differs by {diff} ({})",
                pixel % 4,
                result.reason
            );
        }
        let mut painter = Painter::with_threads(
            &mut expected,
            size.0,
            size.1,
            crate::app::benchmark::threads(),
        )
        .unwrap();
        painter.repair(&result.repair);
        oracle.paint(
            sample.snapshot.frame(),
            &mut painter,
            result.repair.bounds(),
            Color::rgb(0x121419),
        );
        painter.finish().unwrap();
        for (pixel, (a, b)) in target.pixels().iter().zip(&expected).enumerate() {
            if a.abs_diff(*b) <= 1 {
                continue;
            }
            let x = (pixel / 4 % size.0 as usize) as u32;
            let y = (pixel / 4 / size.0 as usize) as u32;
            if !result.repair.intersects(Rect {
                x,
                y,
                width: 1,
                height: 1,
            }) {
                continue;
            }
            assert!(
                a.abs_diff(*b) <= 1,
                "{policy:?}, {buffers} buffers, frame {index}: same-clip pixel {pixel} differs by {}",
                a.abs_diff(*b)
            );
        }
    }
    if reused == 0 {
        let sample = &trace[1];
        let previous = trace[0].snapshot.frame();
        let nodes: Vec<_> = sample
            .snapshot
            .frame()
            .instances()
            .iter()
            .filter(|i| !i.visible_bounds().is_empty())
            .take(6)
            .map(|i| {
                let old = previous.instances().iter().find(|old| old.key == i.key);
                (
                    i.key,
                    i.origin(),
                    i.visible_bounds(),
                    old.map(|old| (old.origin(), old.program.id() == i.program.id())),
                )
            })
            .collect();
        panic!(
            "{policy:?}: no copied pixels; {reasons:?}, motion {:?} -> {:?}, nodes {nodes:?}",
            trace[0].motion.origin, sample.motion.origin
        );
    }
    (error, reused)
}

#[test]
fn target_pixel_reuse_matches_relic_scroll() {
    let frames = trace_samples(1.25, 12);
    for policy in [Policy::Compiled, Policy::Items, Policy::Layers] {
        for buffers in [1, 3] {
            let (_, reused) = validate_reuse(&frames, policy, buffers);
            assert!(
                reused > 0,
                "{policy:?}: exercise pixel reuse, not only fallbacks"
            );
        }
    }
}

fn cpu() -> f64 {
    let t = clock_gettime(ClockId::ProcessCPUTime);
    t.tv_sec as f64 * 1000.0 + t.tv_nsec as f64 / 1_000_000.0
}

#[test]
#[ignore = "completed-target translation experiment; no compositor or game timing"]
fn target_reuse_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("GUI_BENCH_OUTPUT");
    let mut reports = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        let trace = trace_samples(scale, 32);
        let size = trace[0].snapshot.frame().size();
        let policies = if std::env::var_os("GUI_BENCH_REVERSE").is_some() {
            [Policy::Layers, Policy::Items, Policy::Compiled]
        } else {
            [Policy::Compiled, Policy::Items, Policy::Layers]
        };
        for policy in policies {
            for buffers in [1, 3] {
                let (error, reused) = validate_reuse(&trace, policy, buffers);
                assert!(reused > 0);
                for reuse in if std::env::var_os("GUI_BENCH_REVERSE").is_some() {
                    [true, false]
                } else {
                    [false, true]
                } {
                    let mut targets: Vec<_> = (0..buffers).map(|_| Target::default()).collect();
                    let mut renderer = policy.renderer();
                    let mut samples = Vec::new();
                    for cycle in 0..4 {
                        for (index, frame) in trace.iter().enumerate() {
                            let start = cpu();
                            let sample = run(
                                &mut targets[(cycle * trace.len() + index) % buffers],
                                frame,
                                &mut renderer,
                                policy,
                                reuse,
                            );
                            let process = cpu() - start;
                            samples.push(json!({
                                "cycle":cycle,"frame":index,"total_ms":sample.total_ms,
                                "plan_ms":sample.plan_ms,"copy_ms":sample.copy_ms,"prepare_ms":sample.prepare_ms,
                                "submit_ms":sample.submit_ms,"finish_ms":sample.finish_ms,"process_cpu_ms":process,
                                "copied_pixels":sample.copied_pixels,"repaired_pixels":sample.repaired_pixels,
                                "metadata_bytes":sample.metadata_bytes,"reason":sample.reason,
                                "replayed_commands":renderer.stats.replayed_commands,"blits":renderer.stats.blits,
                                "raster_bytes":renderer.residency_stats().live_bytes,
                            }));
                        }
                    }
                    let mut warm: Vec<_> = samples
                        .iter()
                        .filter(|s| s["cycle"] != 0)
                        .map(|s| s["total_ms"].as_f64().unwrap())
                        .collect();
                    warm.sort_by(f64::total_cmp);
                    reports.push(json!({"scale":scale,"policy":format!("{policy:?}"),"reuse":reuse,
                        "buffers":buffers,"size":size,"threads":crate::app::benchmark::threads().get(),
                        "max_pixel_error":error,"additional_pixel_bytes":0,
                        "target_bytes":buffers*size.0 as usize*size.1 as usize*4,
                        "p50_ms":warm[warm.len()/2],"p95_ms":warm[(warm.len()-1)*95/100],"samples":samples}));
                }
            }
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&json!({"schema":1,
        "workload":"relic-scroll-change-reorder-target-reuse",
        "resources":"fixture glyphs, shapes and embedded icons; no remote artwork",
        "note":"Baseline repairs old/new viewport union; reuse copies overlap then repairs exposed strips and dirty holes. Both include preparation and completion. No presentation timing.",
        "results":reports})).unwrap()).unwrap();
}
