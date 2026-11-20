use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, StyleProperty};
use retinue_render::{
    Color, HostPolicy, PaintFrame, PaintInstance, ProgramStore, RasterMode, Rect, RenderHost,
    Renderer, ResidencyPolicy, painter::Painter, text::Blob,
};
use retinue_ui::drawing::text::shaping::ShapedText;
use serde_json::json;
use std::{hint::black_box, sync::Arc, time::Instant};

#[allow(dead_code)]
#[path = "../../../tools/benchmarks/support.rs"]
mod bench;

const FONT: &[u8] = include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf");
const SIZE: (u32, u32) = (1720, 460);
const FONT_SIZE: f32 = 16.0;

fn fonts() -> FontContext {
    let mut fonts = FontContext::new();
    fonts
        .collection
        .register_fonts(Blob::new(Arc::new(FONT)), None);
    fonts
}

fn layout(text: &str, fonts: &mut FontContext, context: &mut LayoutContext<()>) -> ShapedText {
    let mut builder = context.ranged_builder(fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily("Roboto".into()));
    builder.push_default(StyleProperty::FontSize(FONT_SIZE));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());
    ShapedText::from_layout(text, layout).unwrap()
}

#[test]
#[ignore = "text migration experiment; use make bench BENCH_SUITE=text-pipeline"]
fn text_pipeline_benchmark() {
    let mut fonts = fonts();
    let mut context = LayoutContext::<()>::new();
    let labels: Vec<_> = (0..48)
        .map(|i| format!("Galvanized Aptitude {i}"))
        .collect();
    let started = Instant::now();
    let texts: Vec<_> = labels
        .iter()
        .map(|s| layout(s, &mut fonts, &mut context))
        .collect();
    let resolve_ns = started.elapsed().as_nanos() as u64;
    let changed: Vec<_> = (0..4)
        .map(|i| {
            layout(
                &format!("Galvanized Aptitude {i}!"),
                &mut fonts,
                &mut context,
            )
        })
        .collect();
    let threads = bench::render_threads();
    let mut cases = Vec::new();
    if bench::selected("shape-and-resolve") {
        cases.push(bench::measure(
            "shape-and-resolve",
            (0, 0),
            1.0,
            || (fonts, context),
            |(fonts, context), i| {
                black_box(layout(&labels[i % labels.len()], fonts, context));
            },
            |_| json!({"labels_initial_resolve_ns": resolve_ns, "labels": labels.len()}),
        ));
    }
    for scale in [1.0f32, 1.2] {
        if bench::selected("native-repaint") {
            let mut submission = Vec::new();
            let mut completion = Vec::new();
            let native = bench::measure(
                "native-repaint",
                SIZE,
                scale,
                || {
                    (
                        vec![0; (SIZE.0 * SIZE.1 * 4) as usize],
                        texts
                            .iter()
                            .map(|t| t.prepare().unwrap())
                            .collect::<Vec<_>>(),
                    )
                },
                |(pixels, prepared), frame| {
                    let started = Instant::now();
                    let mut painter =
                        Painter::with_threads(pixels, SIZE.0, SIZE.1, threads).unwrap();
                    painter.clear();
                    for (i, text) in prepared.iter().enumerate() {
                        let origin = (
                            ((i % 8) as f64 * 174.0 + 4.0) * scale as f64,
                            ((i / 8) as f64 * 48.0 + 12.0) * scale as f64,
                        );
                        text.draw(&mut painter, origin, scale as f64, Color::rgb(0xeeeeee));
                    }
                    let submitted = started.elapsed().as_nanos() as u64;
                    let finish = Instant::now();
                    painter.finish().unwrap();
                    if frame >= 6 {
                        submission.push(submitted);
                        completion.push(finish.elapsed().as_nanos() as u64);
                    }
                },
                |_| {
                    json!({"labels": labels.len(), "glyphs": texts.iter().flat_map(|t| t.runs())
                .map(|r| r.glyphs().len()).sum::<usize>()})
                },
            );
            let mut native = native;
            native["submission_ns"] = json!(submission);
            native["completion_ns"] = json!(completion);
            cases.push(native);
        }
        for (name, reuse, changing, host_policy) in [
            ("retained-replay", false, false, None),
            ("retained-reuse", true, false, None),
            ("retained-changing-replay", false, true, None),
            ("retained-changing-reuse", true, true, None),
            ("desktop-reuse", true, false, Some(HostPolicy::desktop())),
            ("overlay-reuse", true, false, Some(HostPolicy::overlay())),
            (
                "desktop-changing-reuse",
                true,
                true,
                Some(HostPolicy::desktop()),
            ),
            (
                "overlay-changing-reuse",
                true,
                true,
                Some(HostPolicy::overlay()),
            ),
        ] {
            if !bench::selected(name) {
                continue;
            }
            let clip = Rect {
                width: SIZE.0,
                height: SIZE.1,
                ..Rect::default()
            };
            let warmup_frames = if host_policy.is_some() { 64 } else { 5 };
            let mut startup = Vec::new();
            let mut measured = bench::measure_with_warmup(
                name,
                SIZE,
                scale,
                warmup_frames,
                || {
                    let mut store = ProgramStore::default();
                    let mut states = Vec::new();
                    for variant in 0..=usize::from(changing) {
                        let items = texts
                            .iter()
                            .enumerate()
                            .map(|(i, text)| {
                                let text = if variant != 0 {
                                    changed.get(i).unwrap_or(text)
                                } else {
                                    text
                                };
                                let origin =
                                    ((i % 8) as f32 * 174.0 + 4.0, (i / 8) as f32 * 48.0 + 12.0);
                                let program = store
                                    .record(
                                        i as u64 + 1,
                                        (SIZE.0 as f32 / scale, SIZE.1 as f32 / scale),
                                        scale,
                                        Arc::new(
                                            text.commands(origin, Color::rgb(0xeeeeee)).collect(),
                                        ),
                                    )
                                    .unwrap();
                                PaintInstance::new((i as u64 + 1).into(), program, (0, 0), clip)
                            })
                            .collect::<Vec<_>>();
                        states.push(items);
                    }
                    let mut frame = PaintFrame::default();
                    frame
                        .replace(SIZE, scale, states[0].clone(), Vec::new())
                        .unwrap();
                    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
                    if let Some(policy) = host_policy {
                        renderer.set_host(RenderHost::new(policy));
                    }
                    renderer.set_optimizations(reuse);
                    (
                        vec![0; (SIZE.0 * SIZE.1 * 4) as usize],
                        frame,
                        renderer,
                        states,
                    )
                },
                |(pixels, frame, renderer, states), index| {
                    let started = (index <= warmup_frames).then(Instant::now);
                    if changing {
                        frame
                            .replace(SIZE, scale, states[index % 2].clone(), Vec::new())
                            .unwrap();
                    }
                    renderer.prepare(frame, RasterMode::Explicit);
                    let mut painter =
                        Painter::with_threads(pixels, SIZE.0, SIZE.1, threads).unwrap();
                    painter.clear();
                    renderer.draw_with(frame, &mut painter, clip, |_, _| unreachable!());
                    painter.finish().unwrap();
                    if let Some(started) = started {
                        startup.push((
                            started.elapsed().as_nanos() as u64,
                            renderer.stats.prepared_fonts,
                            renderer.stats.prepared_fragments,
                            renderer.stats.fragment_blits,
                            renderer.stats.deferred_preparations,
                            renderer.host().stats().preparation_time.as_nanos() as u64,
                        ));
                    }
                },
                |(_, _, renderer, _)| {
                    assert!(
                        !renderer.has_deferred(),
                        "preparation did not settle during warm-up"
                    );
                    json!({"labels":labels.len(), "prepared_fonts":renderer.stats.prepared_fonts,
                    "fragment_blits":renderer.stats.fragment_blits,
                    "replayed_commands":renderer.stats.replayed_commands,
                    "retained_raster_bytes":renderer.residency_stats().live_bytes,
                    "reuse":reuse, "changing_labels": if changing { 4 } else { 0 }})
                },
            );
            measured["startup_frames"] = json!(
                startup
                    .iter()
                    .map(
                        |&(elapsed_ns, fonts, fragments, blits, deferred, task_ns)| {
                            json!({"elapsed_ns":elapsed_ns, "prepared_fonts":fonts,
                    "prepared_fragments":fragments, "fragment_blits":blits,
                    "deferred_preparations":deferred, "task_ns":task_ns})
                        }
                    )
                    .collect::<Vec<_>>()
            );
            measured["preparation_policy"] = host_policy.map_or(json!(null), |p| {
                json!({"tasks":p.preparation_tasks, "pixels":p.preparation_pixels,
                    "time_ns":p.preparation_time.as_nanos() as u64})
            });
            cases.push(measured);
        }
    }
    bench::report_with_scope(
        "text",
        cases,
        "Text-only experiment, 48 labels, same strings and font. Shape case includes layout, ink bounds and run resolution. Native submission can auto-flush; completion is the final flush only. Retained cases include renderer preparation/publication/replay and automatic reuse, no cache hints; compilation is in setup. Changing cases alternate four precompiled labels every frame, excluding reshaping. Standalone retained cases have unlimited preparation; desktop/overlay cases use their real preparation budgets with longer recorded warm-up. All cases use BENCH_THREADS for the final pass, not the host's default final-pass count. Queued participants include caller. No Wayland or game.",
    );
}
