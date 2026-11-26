use super::*;
use serde_json::json;

#[allow(dead_code)]
mod bench {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/benchmarks/support.rs"
    ));
}

fn edited(row: bool) -> Element {
    let mut children = vec![child(2)];
    if row {
        children.push(child(3));
    }
    Element::row(1, 0.0, children)
}

#[test]
#[ignore = "structural projection measurement; use make bench BENCH_SUITE=scene-structure"]
fn scene_structure_benchmark() {
    let mut cases = Vec::new();
    for (unrelated, edit_first) in [32, 128, 512]
        .into_iter()
        .flat_map(|n| [(n, false), (n, true)])
    {
        let name = if edit_first {
            format!("shifted-{unrelated}")
        } else {
            format!("mounted-{unrelated}")
        };
        if !bench::selected(&name) {
            continue;
        }
        let size = (32768, 90);
        let mut case = bench::measure(
            &name,
            size,
            1.0,
            || {
                let mut scene = Scene::default();
                let siblings = Element::row(4, 0.0, (0..unrelated).map(|i| child(5 + i)).collect());
                let children = if edit_first {
                    vec![edited(false), siblings]
                } else {
                    vec![siblings, edited(false)]
                };
                scene
                    .update(
                        Element::column(0, 0.0, children),
                        size,
                        1.0,
                        RecordingMode::Retained,
                    )
                    .unwrap();
                scene
            },
            |scene, i| {
                scene.update_subtree(edited(i % 2 == 0)).unwrap();
            },
            |scene| {
                json!({"unrelated_leaves":unrelated, "restored_records":scene.stats.restored_records,
                "recordings":scene.stats.recordings, "layout_computations":scene.stats.layout_computations})
            },
        );
        case["actual_threads"] = json!(1);
        cases.push(case);
    }
    bench::report_with_scope(
        "scene-structure",
        cases,
        "Insert or remove one painted leaf below a scoped child. The unchanged sibling precedes the edit (mounted) or follows it (shifted). Includes reconciliation, layout and frame publication; no drawing. Restored records count the retained geometry and placement pairs republished in the last update.",
    );
}
