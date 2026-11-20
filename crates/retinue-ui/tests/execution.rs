use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, StyleProperty};
use retinue_render::{
    Color,
    painter::RasterImage,
    text::Blob,
    work::{Executor, Graph, Limits, Priority},
};
use retinue_ui::drawing::text::shaping::ShapedText;
use serde_json::json;
use std::{
    cell::RefCell,
    num::NonZeroUsize,
    sync::{Arc, OnceLock, mpsc},
    time::Duration,
};

#[allow(dead_code)]
#[path = "../../../tools/benchmarks/support.rs"]
mod bench;

const FONT: &[u8] = include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf");
static FONT_DATA: OnceLock<Blob<u8>> = OnceLock::new();
thread_local! {
    static SHAPER: RefCell<(FontContext, LayoutContext<()>)> = RefCell::new({
        let mut fonts = FontContext::new();
        fonts.collection.register_fonts(FONT_DATA.get_or_init(|| Blob::new(Arc::new(FONT))).clone(), None);
        (fonts, LayoutContext::new())
    });
}

fn shape(index: usize, skewed: bool) -> ShapedText {
    let text = if skewed && index % 4 == 0 {
        "Galvanized Aptitude: +80% Status Chance. On Kill: +40% Direct Damage per Status Type affecting the target for 20s. Stacks up to 2x. ".repeat(4)
    } else {
        format!("Galvanized Aptitude {index}")
    };
    SHAPER.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        let (fonts, context) = &mut *scratch;
        let mut builder = context.ranged_builder(fonts, &text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily("Roboto".into()));
        builder.push_default(StyleProperty::FontSize(16.0));
        let mut layout = builder.build(&text);
        layout.break_all_lines(Some(208.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        ShapedText::from_layout(&text, layout).unwrap()
    })
}

fn tile(shape: &ShapedText) -> RasterImage {
    let prepared = shape.prepare().unwrap();
    RasterImage::render(224, 144, |painter| {
        painter.fill_rounded_rect(0, 0, 224, 144, 6, [33, 29, 27, 255]);
        prepared.draw(painter, (8.0, 8.0), 1.0, Color::rgb(0xeeeeee));
    })
    .unwrap()
}

fn row<'a>(images: impl Iterator<Item = &'a RasterImage>) -> RasterImage {
    RasterImage::render(896, 144, |painter| {
        for (index, image) in images.enumerate() {
            painter.draw_image(image, index as u32 * 224, 0, 224, 144);
        }
    })
    .unwrap()
}

fn serial(count: usize, skewed: bool) -> Vec<RasterImage> {
    let shapes: Vec<_> = (0..count).map(|i| shape(i, skewed)).collect();
    let tiles: Vec<_> = shapes.iter().map(tile).collect();
    tiles.chunks(4).map(|images| row(images.iter())).collect()
}

fn parallel(pool: &Executor, count: usize, skewed: bool, batch: usize) -> Vec<RasterImage> {
    let mut graph = Graph::default();
    let mut tiles = Vec::new();
    for start in (0..count).step_by(batch) {
        let end = (start + batch).min(count);
        let shapes = graph.add(&[], NonZeroUsize::MIN, move |_| {
            Ok((start..end).map(|i| shape(i, skewed)).collect::<Vec<_>>())
        });
        tiles.push(graph.add(&[shapes.id()], NonZeroUsize::MIN, move |_| {
            Ok(shapes
                .read()
                .unwrap()
                .unwrap()
                .iter()
                .map(tile)
                .collect::<Vec<_>>())
        }));
    }
    let mut rows = Vec::new();
    for group in tiles.chunks(4 / batch) {
        let dependencies: Vec<_> = group.iter().map(|output| output.id()).collect();
        let group = group.to_vec();
        rows.push(graph.add(&dependencies, NonZeroUsize::MIN, move |_| {
            Ok(row(group
                .iter()
                .flat_map(|output| output.read().unwrap().unwrap().iter())))
        }));
    }
    drop(tiles);
    let (send, receive) = mpsc::channel();
    let submission = pool
        .submit(graph, Priority::Foreground, move || {
            let _ = send.send(());
        })
        .unwrap();
    receive.recv_timeout(Duration::from_secs(10)).unwrap();
    let report = submission.try_complete().unwrap();
    assert_eq!(report.failed + report.cancelled, 0);
    rows.into_iter()
        .map(|output| output.into_result().ok().unwrap().unwrap())
        .collect()
}

fn pixels(images: &[RasterImage]) -> Vec<Vec<u8>> {
    images
        .iter()
        .map(|image| image.to_rgba().unwrap().into_raw())
        .collect()
}

fn pool(participants: usize) -> Executor {
    Executor::new(Limits {
        participants: NonZeroUsize::new(participants).unwrap(),
        tasks: 256,
    })
}

#[test]
fn shaping_raster_and_composite_dag_matches_serial_at_both_granularities() {
    for skewed in [false, true] {
        let expected = pixels(&serial(8, skewed));
        for participants in [1, 2, 4] {
            let pool = pool(participants);
            for batch in [1, 4] {
                assert_eq!(pixels(&parallel(&pool, 8, skewed, batch)), expected);
            }
        }
    }
}

#[test]
#[ignore = "real CPU DAG benchmark; use make bench BENCH_SUITE=execution"]
fn execution_benchmark() {
    let participants = bench::render_threads().get() as usize;
    let mut cases = Vec::new();
    for (label, count, skewed) in [
        ("small", 4, false),
        ("cards", 48, false),
        ("skewed", 48, true),
    ] {
        for batch in [0, 1, 4] {
            let mode = match batch {
                0 => "serial",
                1 => "jobs",
                _ => "batches",
            };
            let name = format!("{label}-{mode}");
            if !bench::selected(&name) {
                continue;
            }
            let expected = pixels(&serial(count, skewed));
            let measured = bench::measure_with_warmup(
                &name,
                (896, (count / 4 * 144) as u32),
                1.0,
                10,
                || (pool(participants), Vec::new()),
                |(pool, result), _| {
                    *result = if batch == 0 {
                        serial(count, skewed)
                    } else {
                        parallel(pool, count, skewed, batch)
                    };
                },
                |(_, result)| {
                    assert_eq!(pixels(result), expected);
                    json!({"tiles": count, "batch": batch, "skewed": skewed, "participants": participants,
                        "native_participants_per_job": 1, "output_bytes": result.iter().map(RasterImage::byte_len).sum::<usize>()})
                },
            );
            cases.push(measured);
        }
    }
    bench::report_with_scope(
        "execution",
        cases,
        "CPU DAG build, shaping, native raster and row composition; completion wait on benchmark caller only; no presentation",
    );
}
