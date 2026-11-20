use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, StyleProperty};
use retinue_render::text::Blob;
use retinue_ui::drawing::text::shaping::ShapedText;
use serde_json::json;
use std::{hint::black_box, sync::Arc};

#[allow(dead_code)]
#[path = "../../../tools/benchmarks/support.rs"]
mod bench;

const FONT: &[u8] = include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf");
const TEXT: &str = "Galvanized Aptitude: direct damage per status type affecting the target.\n\nOffice cafe\u{301}: combining marks and ligatures share the measured and painted layout. Fractional widths must not reshape this paragraph.";
const WIDTHS: [f32; 4] = [180.5, 320.25, 240.75, 105.125];

fn context() -> (FontContext, LayoutContext<()>) {
    let mut fonts = FontContext::new();
    fonts
        .collection
        .register_fonts(Blob::new(Arc::new(FONT)), None);
    (fonts, LayoutContext::new())
}

fn shape(fonts: &mut FontContext, context: &mut LayoutContext<()>, width: f32) -> ShapedText {
    let mut builder = context.ranged_builder(fonts, TEXT, 1.0, false);
    builder.push_default(StyleProperty::FontFamily("Roboto".into()));
    builder.push_default(StyleProperty::FontSize(16.0));
    let mut layout = builder.build(TEXT);
    layout.break_all_lines(Some(width));
    layout.align(Alignment::Start, AlignmentOptions::default());
    ShapedText::from_layout(TEXT, layout).unwrap()
}

#[test]
fn paragraph_reflow_matches_fresh_shaping() {
    let (mut fonts, mut context) = context();
    let mut retained = shape(&mut fonts, &mut context, WIDTHS[0]);
    for width in WIDTHS.into_iter().cycle().take(12) {
        retained = retained
            .reflow(Some(width), Alignment::Start, AlignmentOptions::default())
            .unwrap();
        let fresh = shape(&mut fonts, &mut context, width);
        assert_eq!(retained.layout().width(), fresh.layout().width());
        assert_eq!(retained.layout().height(), fresh.layout().height());
        let geometry = |text: &ShapedText| {
            text.layout()
                .lines()
                .map(|line| (line.text_range(), *line.metrics()))
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry(&retained), geometry(&fresh));
        assert_eq!(retained.runs(), fresh.runs());
    }
}

#[test]
#[ignore = "text layout measurement; use make bench BENCH_SUITE=text-layout"]
fn text_layout_benchmark() {
    let mut cases = Vec::new();
    for (name, retain) in [("paragraph-reshape", false), ("paragraph-reflow", true)] {
        if !bench::selected(name) {
            continue;
        }
        let mut case = bench::measure(
            name,
            (0, 0),
            1.0,
            || {
                let (mut fonts, mut context) = context();
                let text = shape(&mut fonts, &mut context, WIDTHS[0]);
                (fonts, context, Some(text))
            },
            |(fonts, context, text), index| {
                let width = WIDTHS[index % WIDTHS.len()];
                *text = Some(if retain {
                    text.take()
                        .unwrap()
                        .reflow(Some(width), Alignment::Start, AlignmentOptions::default())
                        .unwrap()
                } else {
                    shape(fonts, context, width)
                });
                black_box(text.as_ref().unwrap().runs());
            },
            |(_, _, text)| {
                json!({
                    "source_bytes": TEXT.len(), "widths": WIDTHS,
                    "glyphs": text.as_ref().unwrap().runs().iter().map(|run| run.glyphs().len()).sum::<usize>(),
                    "text_layout": "one mutable layout; shared immutable recorded glyph arrays",
                })
            },
        );
        case["actual_threads"] = json!(1);
        case["items"] = json!(1);
        cases.push(case);
    }
    bench::report_with_scope(
        "native-text-layout",
        cases,
        "Warm shaper resources; one paragraph per sample. Reshape includes native face validation; reflow retains validated faces and changes line placement without shaping. Both resolve positioned glyph arrays. No painting, native contexts, input or platform presentation.",
    );
}
