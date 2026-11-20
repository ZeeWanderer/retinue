use retinue_render::{Color, Command};
use retinue_ui::{
    authoring::Text,
    environment::Environment,
    ir::InstanceGraph,
    prelude::{TextStyle, Typography},
};
use serde_json::json;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "../../../tools/benchmarks/support.rs"]
mod bench;

const TEXT: &str = "Galvanized Aptitude: direct damage per status type affecting the target.\n\nOffice cafe\u{301}: combining marks and ligatures share the measured and painted layout.";
const WIDTHS: [u32; 4] = [180, 320, 241, 105];

fn view(width: u32, limit: Option<usize>) -> (InstanceGraph, Environment) {
    let text = Text::new(TEXT);
    let text = limit.map_or(text.clone(), |limit| text.line_limit(limit));
    let environment = Environment::default();
    let mut tree =
        InstanceGraph::with_root_body(move |_| text.clone(), environment.clone()).unwrap();
    tree.update_inline((width, 480), 1.0).unwrap();
    (tree, environment)
}

#[test]
#[ignore = "Text primitive measurement; use make bench BENCH_SUITE=text-view"]
fn text_view_benchmark() {
    let mut cases = Vec::new();
    for name in ["text-fresh", "text-width", "text-color", "text-elision"] {
        if !bench::selected(name) {
            continue;
        }
        let limit = (name == "text-elision").then_some(2);
        let mut case = bench::measure(
            name,
            (320, 480),
            1.0,
            || view(320, limit),
            |host, index| {
                let width = WIDTHS[index % WIDTHS.len()];
                match name {
                    "text-fresh" => *host = view(width, limit),
                    "text-color" => {
                        let mut style = TextStyle::default();
                        style.color = Color::rgb(if index % 2 == 0 { 0xddbb99 } else { 0xffffff });
                        host.1.set::<Typography>(style);
                        host.0.update_inline((320, 480), 1.0).unwrap();
                    }
                    _ => {
                        host.0.update_inline((width, 480), 1.0).unwrap();
                    }
                }
                black_box(host.0.frame());
            },
            |(tree, _)| {
                if name == "text-color" {
                    assert_eq!(tree.scene().stats.layout_computations, 0);
                }
                let glyphs = tree
                    .frame()
                    .instances()
                    .iter()
                    .flat_map(|item| item.program.commands().iter())
                    .map(|command| match command {
                        Command::GlyphRun { run, .. } => run.glyphs().len(),
                        _ => panic!("Text must use native glyph runs"),
                    })
                    .sum::<usize>();
                json!({
                    "source_bytes": TEXT.len(), "widths": WIDTHS, "glyphs": glyphs,
                    "layout_computations": tree.scene().stats.layout_computations,
                    "recordings": tree.scene().stats.recordings,
                })
            },
        );
        case["actual_threads"] = json!(1);
        case["items"] = json!(1);
        cases.push(case);
    }
    bench::report_with_scope(
        "native-text-view",
        cases,
        "One compiled Text update per sample, including layout and retained command recording. Font bytes and registration are warm. Fresh replaces the graph; width and elision retain it; color changes neither shaping nor layout. No painting, native contexts or presentation.",
    );
}
