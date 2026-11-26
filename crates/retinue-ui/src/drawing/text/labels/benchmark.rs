use super::*;
use retinue_render::painter::load_overlay_font;
use serde_json::json;

#[allow(dead_code)]
#[path = "../../../../../../tools/benchmarks/support.rs"]
mod bench;

#[test]
#[ignore = "label placement measurement; use make bench BENCH_SUITE=label-placement"]
fn label_placement_benchmark() {
    let text = "Cold resistance: equipment modifier (Tier 3)";
    let mut cases = Vec::new();
    for (name, width, twice) in [
        ("direct", None, false),
        ("double", None, true),
        ("wrapped-direct", Some(95.0), false),
        ("wrapped-double", Some(95.0), true),
    ] {
        if !bench::selected(name) {
            continue;
        }
        let align = if width.is_some() {
            Alignment::Center
        } else {
            Alignment::Start
        };
        let mut case = bench::measure(
            name,
            (0, 0),
            1.0,
            || TextFace::for_font(&load_overlay_font().unwrap()).unwrap(),
            |face, _| {
                let result = if twice {
                    face.shape(text, 14.0)
                        .unwrap()
                        .reflow(width, align, Default::default())
                        .unwrap()
                } else {
                    face.shape_at(text, 14.0, width, Some(align)).unwrap()
                };
                std::hint::black_box(result);
            },
            |_| json!({"source_bytes":text.len(), "width":width}),
        );
        case["actual_threads"] = json!(1);
        cases.push(case);
    }
    bench::report_with_scope(
        "label-placement",
        cases,
        "One uncached label's layout and glyph resolution, warm font/shaper. Double resolves unbounded runs before final placement. No painting or label cache lookup.",
    );
}
