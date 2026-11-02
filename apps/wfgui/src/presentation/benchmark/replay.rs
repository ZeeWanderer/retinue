use super::*;
use retinue_render::{Color, FrameSnapshot, LayerPolicy, PaintFrame, RasterMode, Renderer};

#[derive(Clone, Copy, Debug)]
enum Policy {
    Raw,
    Compiled,
    Fragments,
    Items,
    Layers,
}

impl Policy {
    fn renderer(self) -> Renderer {
        let mut renderer = match self {
            Self::Raw | Self::Compiled | Self::Fragments => {
                Renderer::new(retinue_render::ResidencyPolicy::desktop(0))
            }
            Self::Items => {
                Renderer::new(retinue_render::ResidencyPolicy::desktop(16 * 1024 * 1024))
            }
            Self::Layers => Renderer::with_layers(
                retinue_render::ResidencyPolicy::desktop(16 * 1024 * 1024),
                LayerPolicy::desktop(),
            ),
        };
        renderer.set_optimizations(!matches!(self, Self::Raw));
        renderer.set_materializations(!matches!(self, Self::Raw | Self::Compiled));
        renderer
    }

    fn mode(self) -> RasterMode {
        match self {
            Self::Raw | Self::Compiled | Self::Fragments => RasterMode::Replay,
            Self::Items | Self::Layers => RasterMode::Explicit,
        }
    }
}

fn trace(scale: f32, frames: usize) -> Vec<FrameSnapshot> {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    let source = serde_json::to_value(&pages.relics.items[0]).unwrap();
    let rows: Vec<_> = (0..120)
        .map(|i| {
            let mut row = source.clone();
            row["id"] = json!(format!("replay-{i}"));
            row["name"] = json!(format!("Lith {i:03}"));
            row["rewards"] = json!(
                (0..6)
                    .map(|j| source["rewards"][j % 2].clone())
                    .collect::<Vec<_>>()
            );
            row
        })
        .collect();
    pages
        .relics
        .replace_fixture(json!({"items": rows}), &pages.quotes);
    pages.set_page(Page::Relics);
    pages.configure((1800, 1360), (400, 1360), scale);
    let mut target = PaintFrame::default();
    let size = pages.ui.size;
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut frames_out = Vec::with_capacity(frames);
    let now = Instant::now();
    for step in 0..frames {
        let time = now + std::time::Duration::from_millis(step as u64 * 16);
        let offset = if step < frames / 2 {
            step
        } else {
            frames - step - 1
        } as f32
            * 9.0;
        pages.relics.scroll.to(offset, false, time);
        pages.relics.tick(time);
        if step == frames / 3 {
            pages.relics.items[1].name = "Updated relic".into();
        }
        if step == frames * 2 / 3 {
            pages.relics.items.swap(1, 4);
        }
        pages.dirty = true;
        pages.layout();
        let mut painter = Painter::with_threads(
            &mut pixels,
            size.0,
            size.1,
            crate::app::benchmark::threads(),
        )
        .unwrap();
        pages.paint(&mut painter);
        painter.finish().unwrap();
        frames_out.push(
            pages
                .ui
                .paint_snapshot(&mut target, pages.ui.bounds("page.grid").unwrap()),
        );
    }
    frames_out
}

fn render(
    frame: &PaintFrame,
    renderer: &mut Renderer,
    policy: Policy,
    pixels: &mut [u8],
) -> (f64, f64, f64) {
    let start = Instant::now();
    renderer.prepare(frame, policy.mode());
    let prepared = Instant::now();
    let size = frame.size();
    let mut painter =
        Painter::with_threads(pixels, size.0, size.1, crate::app::benchmark::threads()).unwrap();
    renderer.paint(
        frame,
        &mut painter,
        Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        },
        Color::rgb(0x121419),
    );
    let submitted = Instant::now();
    painter.finish().unwrap();
    (
        (prepared - start).as_secs_f64() * 1000.0,
        (submitted - prepared).as_secs_f64() * 1000.0,
        submitted.elapsed().as_secs_f64() * 1000.0,
    )
}

fn validate(trace: &[FrameSnapshot], policy: Policy) -> u8 {
    let size = trace[0].frame().size();
    let mut actual = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut expected = actual.clone();
    let mut renderer = policy.renderer();
    let mut oracle = Policy::Raw.renderer();
    let mut error = 0;
    let tolerance = match policy {
        Policy::Raw | Policy::Compiled | Policy::Fragments => 0,
        Policy::Items | Policy::Layers => 1,
    };
    for snapshot in trace {
        render(snapshot.frame(), &mut renderer, policy, &mut actual);
        render(snapshot.frame(), &mut oracle, Policy::Raw, &mut expected);
        let glows = if matches!(policy, Policy::Raw | Policy::Compiled) {
            Vec::new()
        } else {
            glow_regions(snapshot.frame())
        };
        for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
            let diff = a.abs_diff(*b);
            error = error.max(diff);
            if diff <= tolerance {
                continue;
            }
            let x = (index / 4) as u32 % size.0;
            let y = (index / 4) as u32 / size.0;
            let glow = glows
                .iter()
                .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height);
            assert!(
                glow && diff <= 3,
                "{policy:?}: replay pixel error {diff} at {x},{y}"
            );
        }
    }
    error
}

fn glow_regions(frame: &PaintFrame) -> Vec<Rect> {
    use retinue_render::{Command, PaintExtent};
    fn walk(commands: &[Command], scale: f32, origin: (i32, i32), clip: Rect, out: &mut Vec<Rect>) {
        for command in commands {
            match command {
                Command::Fragment { program, .. } => {
                    // Apply the separately tested glow-compositing tolerance only to its pixels.
                    if program
                        .commands()
                        .iter()
                        .all(|c| matches!(c, Command::InsetShadow { .. }))
                    {
                        out.push(program.analysis().effects.writes.visible(origin, clip));
                    } else {
                        walk(program.commands(), program.scale(), origin, clip, out);
                    }
                }
                Command::Translate { offset, commands } => walk(
                    commands,
                    scale,
                    (
                        origin.0 + (offset.0 * scale).round() as i32,
                        origin.1 + (offset.1 * scale).round() as i32,
                    ),
                    clip,
                    out,
                ),
                Command::Clip { bounds, commands } => {
                    let bounds = PaintExtent {
                        x: (bounds.x * scale).round() as i32,
                        y: (bounds.y * scale).round() as i32,
                        width: (bounds.width * scale).round() as u32,
                        height: (bounds.height * scale).round() as u32,
                    };
                    walk(commands, scale, origin, bounds.visible(origin, clip), out);
                }
                _ => (),
            }
        }
    }
    let mut regions = Vec::new();
    for instance in frame.instances() {
        walk(
            instance.program.commands(),
            frame.scale(),
            instance.origin(),
            instance.clip(),
            &mut regions,
        );
    }
    regions
}

#[test]
fn captured_relic_paint_replays_without_the_page() {
    let frames = trace(1.25, 4);
    validate(&frames, Policy::Compiled);
    validate(&frames, Policy::Fragments);
    validate(&frames, Policy::Items);
}

#[test]
#[ignore = "matched immutable paint-frame replay, without layout or live callbacks"]
fn paint_replay_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let cpu = |clock| {
        let time = clock_gettime(clock);
        time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1e6
    };
    let mut reports = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        let frames = trace(scale, 32);
        let policies = if std::env::var_os("GUI_BENCH_REVERSE").is_some() {
            [
                Policy::Layers,
                Policy::Items,
                Policy::Fragments,
                Policy::Compiled,
                Policy::Raw,
            ]
        } else {
            [
                Policy::Raw,
                Policy::Compiled,
                Policy::Fragments,
                Policy::Items,
                Policy::Layers,
            ]
        };
        for policy in policies {
            let max_error = validate(&frames, policy);
            let size = frames[0].frame().size();
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut renderer = policy.renderer();
            let mut samples = Vec::new();
            for cycle in 0..3 {
                for (index, snapshot) in frames.iter().enumerate() {
                    let process = cpu(ClockId::ProcessCPUTime);
                    let caller = cpu(ClockId::ThreadCPUTime);
                    let start = Instant::now();
                    let (prepare, submit, finish) =
                        render(snapshot.frame(), &mut renderer, policy, &mut pixels);
                    let total = start.elapsed().as_secs_f64() * 1000.0;
                    let caller = cpu(ClockId::ThreadCPUTime) - caller;
                    let process = cpu(ClockId::ProcessCPUTime) - process;
                    samples.push(json!({"cycle":cycle,"frame":index,"prepare_ms":prepare,"submit_ms":submit,"finish_ms":finish,
                        "total_ms":total,"caller_cpu_ms":caller,"process_cpu_ms":process,"blits":renderer.stats.blits,
                        "replayed_commands":renderer.stats.replayed_commands,"prepared_rasters":renderer.stats.prepared_rasters,
                        "preparation_tasks":renderer.stats.preparation_tasks,"preparation_edges":renderer.stats.preparation_edges,
                        "failed_preparations":renderer.stats.failed_preparations,"draw_steps":renderer.execution_plan().len(),
                        "repaired_pixels":renderer.stats.repaired_pixels,"copied_pixels":renderer.stats.copied_pixels,
                        "raster_bytes":renderer.residency_stats().live_bytes,"promotions":renderer.stats.promoted_layers,"demotions":renderer.stats.demoted_layers}));
                }
            }
            let mut warm: Vec<_> = samples
                .iter()
                .filter(|s| s["cycle"] != 0)
                .map(|s| s["total_ms"].as_f64().unwrap())
                .collect();
            warm.sort_by(f64::total_cmp);
            reports.push(json!({"scale":scale,"policy":format!("{policy:?}"),"size":size,"frames":frames.len(),
                "threads":crate::app::benchmark::threads().get(),"max_pixel_error":max_error,
                "p50_ms":warm[warm.len()/2],"p95_ms":warm[(warm.len()-1)*95/100],"p99_ms":warm[(warm.len()-1)*99/100],"max_ms":warm.last(),
                "samples":samples}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&json!({"schema":1,"workload":"relic-scroll-change-reorder",
        "resources":"fixture glyphs, shapes and embedded icons prepared; no remote item artwork","trace_frames":32,"results":reports})).unwrap()).unwrap();
}
