use super::*;
use retinue_render::{
    Bounds, CacheHint, CachePolicy, CollectionContext, CollectionMotion, CollectionRow, Command,
    OptimizationContext, PaintGroup, PaintInstance, PaintKey, ProgramStore,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
enum Coverage {
    Rows,
    Viewport,
}

fn renderer(policy: Policy) -> Renderer {
    if matches!(policy, Policy::Layers) {
        Renderer::with_layers(
            retinue_render::ResidencyPolicy::desktop(16 * 1024 * 1024),
            LayerPolicy {
                threads: crate::app::benchmark::threads().get(),
                ..LayerPolicy::desktop()
            },
        )
    } else {
        policy.renderer()
    }
}

fn trace(coverage: Coverage, scale: f32, steps: usize) -> Vec<TraceFrame> {
    let px = |value: f32| (value * scale).round() as u32;
    let viewport = Rect {
        x: 20,
        y: 20,
        width: 1760,
        height: 1320,
    };
    let columns = (viewport.width / px(350.0)).max(1) as usize;
    let rows = (viewport.height / px(190.0)) as usize + 4;
    let mut store = ProgramStore::default();
    let background_size = (
        viewport.width as f32 / scale,
        viewport.height as f32 / scale,
    );
    let background = store.compile(
        PaintKey::new(10000.into(), 0),
        background_size,
        scale,
        Rc::new(vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, background_size.0, background_size.1),
            radius: 0.0,
            color: Color::rgb(0x121419),
        }]),
    );
    let programs: Vec<_> = (0..rows * columns)
        .map(|id| {
            let commands = (0..96)
                .map(|n| Command::Fill {
                    bounds: Bounds::new((n % 12) as f32 * 28.0, (n / 12) as f32 * 22.0, 26.0, 20.0),
                    radius: 4.0,
                    color: Color::rgba(
                        40 + (id * 11 % 160) as u8,
                        70 + (n * 13 % 160) as u8,
                        130,
                        255,
                    ),
                })
                .collect();
            store.compile(
                PaintKey::new((id as u64).into(), 0),
                (340.0, 180.0),
                scale,
                Rc::new(commands),
            )
        })
        .collect();
    let mut frame = PaintFrame::default();
    (0..steps)
        .map(|step| {
            let phase = step % 48;
            let scroll = (phase.min(47 - phase) as i32) * px(9.0) as i32;
            let origin = (viewport.x as i32, viewport.y as i32 - scroll);
            let mut items: Vec<_> = programs
                .iter()
                .enumerate()
                .map(|(id, program)| {
                    let mut item = PaintInstance::new(
                        (id as u64).into(),
                        program.clone(),
                        (
                            origin.0 + px((id % columns) as f32 * 350.0) as i32,
                            origin.1 + px((id / columns) as f32 * 190.0) as i32,
                        ),
                        viewport,
                    );
                    item.cache = CachePolicy::Never;
                    item
                })
                .collect();
            let mut backdrop = PaintInstance::new(
                10000.into(),
                background.clone(),
                (viewport.x as i32, viewport.y as i32),
                viewport,
            );
            backdrop.cache = CachePolicy::Never;
            items.insert(0, backdrop);
            let mut context = OptimizationContext::default().with(CacheHint::Stable);
            if matches!(coverage, Coverage::Rows) {
                context = context.with(CollectionContext {
                    origin,
                    motion: CollectionMotion::Translating,
                    rows: (0..rows)
                        .map(|row| CollectionRow {
                            key: (row as u64).into(),
                            instances: 1 + row * columns..1 + (row + 1) * columns,
                        })
                        .collect(),
                });
            }
            let count = items.len();
            frame
                .replace(
                    (1800, 1360),
                    scale,
                    items,
                    vec![PaintGroup {
                        key: 1000.into(),
                        instances: usize::from(matches!(coverage, Coverage::Rows))..count,
                        context,
                    }],
                )
                .unwrap();
            TraceFrame {
                snapshot: frame.snapshot(),
                motion: retinue_render::relocate::Motion { viewport, origin },
            }
        })
        .collect()
}

fn prime(targets: &mut [Target], trace: &[TraceFrame], renderer: &mut Renderer, policy: Policy) {
    // An intentionally favourable control: pay cache admission before measuring motion.
    for step in 0..24 {
        let index = step % targets.len();
        run(&mut targets[index], &trace[0], renderer, policy, false);
    }
}

#[test]
fn target_reuse_coexists_with_materialized_rows_and_viewport() {
    for (coverage, scale) in [
        (Coverage::Rows, 1.0),
        (Coverage::Viewport, 1.0),
        (Coverage::Rows, 2.0),
        (Coverage::Viewport, 2.0),
    ] {
        let frames = trace(coverage, scale, 12);
        let mut targets: Vec<_> = (0..3).map(|_| Target::default()).collect();
        let mut renderer = renderer(Policy::Layers);
        prime(&mut targets, &frames, &mut renderer, Policy::Layers);
        assert!(renderer.residency_stats().selected_bytes > 1024 * 1024);
        let size = frames[0].snapshot.frame().size();
        let mut expected = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut oracle = Policy::Raw.renderer();
        let mut copies = 0;
        for (index, frame) in frames.iter().enumerate() {
            let target = &mut targets[index % 3];
            let result = run(target, frame, &mut renderer, Policy::Layers, true);
            copies += usize::from(
                result.copied_pixels > 0 && renderer.residency_stats().selected_bytes > 1024 * 1024,
            );
            render(
                frame.snapshot.frame(),
                &mut oracle,
                Policy::Raw,
                &mut expected,
            );
            let error = target
                .pixels()
                .iter()
                .zip(&expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(error <= 1, "{coverage:?}, frame {index}, error {error}");
        }
        assert!(
            copies > 0,
            "{coverage:?}: reuse must coexist with an admitted cache"
        );
    }
}

#[test]
#[ignore = "synthetic cache-heavy control, not production admission or compositor timing"]
fn target_materialization_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("GUI_BENCH_OUTPUT");
    let reverse = std::env::var_os("GUI_BENCH_REVERSE").is_some();
    let mut reports = Vec::new();
    for scale in [1.0, 2.0] {
        for coverage in [Coverage::Rows, Coverage::Viewport] {
            let trace = trace(coverage, scale, 96);
            let policies = if reverse {
                [Policy::Layers, Policy::Compiled]
            } else {
                [Policy::Compiled, Policy::Layers]
            };
            for buffers in [1, 3] {
                for policy in policies {
                    for reuse in if reverse {
                        [true, false]
                    } else {
                        [false, true]
                    } {
                        let mut renderer = renderer(policy);
                        let mut targets: Vec<_> = (0..buffers).map(|_| Target::default()).collect();
                        prime(&mut targets, &trace, &mut renderer, policy);
                        let initial_raster_bytes = renderer.residency_stats().live_bytes;
                        if matches!(policy, Policy::Layers) {
                            assert!(initial_raster_bytes > 1024 * 1024);
                        }
                        let mut samples = Vec::new();
                        for (index, frame) in trace.iter().enumerate() {
                            let start = cpu();
                            let sample = run(
                                &mut targets[index % buffers],
                                frame,
                                &mut renderer,
                                policy,
                                reuse,
                            );
                            let process = cpu() - start;
                            samples.push(json!({
                                "frame":index,"total_ms":sample.total_ms,"process_cpu_ms":process,
                                "plan_ms":sample.plan_ms,"copy_ms":sample.copy_ms,
                                "prepare_ms":sample.prepare_ms,"submit_ms":sample.submit_ms,"finish_ms":sample.finish_ms,
                                "copied_pixels":sample.copied_pixels,"repaired_pixels":sample.repaired_pixels,
                                "metadata_bytes":sample.metadata_bytes,"reason":sample.reason,
                                "raster_bytes":renderer.residency_stats().live_bytes,
                                "layer_bytes":renderer.stats.layer_bytes,
                                "prepared_rasters":renderer.stats.prepared_rasters,
                            }));
                        }
                        reports.push(json!({"scale":scale,"coverage":format!("{coverage:?}"),
                            "policy":format!("{policy:?}"),"reuse":reuse,"buffers":buffers,
                            "initial_raster_bytes":initial_raster_bytes,"samples":samples}));
                    }
                }
            }
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&json!({"schema":1,
        "workload":"synthetic-cache-heavy-scroll","size":[1800,1360],
        "threads":crate::app::benchmark::threads().get(),
        "note":"Repeated rounded shapes, no remote assets; 24 stationary priming repaints deliberately admit caches. Warm motion excludes the first 3 frames. Measures headless paint and completion, not layout or presentation.",
        "results":reports})).unwrap()).unwrap();
}
