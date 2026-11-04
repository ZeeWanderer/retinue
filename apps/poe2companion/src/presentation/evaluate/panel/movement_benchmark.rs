use super::*;
use crate::render_bench;
use retinue_render::surface::experiment::{Transfer, Trial};

#[test]
#[ignore = "headless drag repair comparison"]
fn window_drag_benchmark() {
    let mut cases = Vec::new();
    for size in [(1920, 1080), (2560, 1440)] {
        for scale in [1.0, 1.5] {
            let mut policies = [
                ("window-drag-full", Transfer::NoRelocate),
                ("window-drag", Transfer::NoRelocate),
                ("window-drag-relocate", Transfer::InPlace),
                ("window-drag-native", Transfer::Previous),
                ("window-drag-regions", Transfer::PreviousRegions),
                ("window-drag-cpu", Transfer::PreviousCpu),
                ("window-drag-cpu-parallel", Transfer::PreviousCpuParallel),
            ];
            if std::env::var_os("RETINUE_BENCH_REVERSE").is_some() {
                policies.reverse();
            }
            for (policy, transfer) in policies {
                if !render_bench::selected(policy) {
                    continue;
                }
                cases.push(render_bench::measure(
                    policy,
                    size,
                    scale,
                    || {
                        let data: serde_json::Value = serde_json::from_slice(include_bytes!(
                            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/evaluate-reference.json")
                        )).unwrap();
                        let mut model = EvaluateModel::new("Forbidden Rites".into());
                        model.evaluated(0, serde_json::from_value(data["evaluation"].clone()).unwrap());
                        model.results = Some(serde_json::from_value(data["results"].clone()).unwrap());
                        let mut view = EvaluatePanel::new().unwrap();
                        view.render_budget = render_bench::render_threads();
                        (view, model, Interaction::default(), Trial::new(size, 3, transfer),
                            0u64, 0u64, 0u64, 0u64)
                    },
                    |(view, model, interaction, repaint, changed, repaired, copied, frames), index| {
                        let previous = view.bounds;
                        let step = (index % 80).min(80 - index % 80) as u32;
                        interaction.position = Some((100 + step * 8, 30 + step));
                        let damage = view.prepare(Frame {size, scale, zoom: scale, phase: 0.0},
                            model, interaction).unwrap().union(Damage::rect(previous));
                        repaint.invalidate(if policy == "window-drag-full" { Damage::Full } else { damage });
                        let frame = view.begin_frame();
                        let plan = repaint.paint(index % 3, view.paint_scene().unwrap(), frame.threads(), |painter, repair| {
                            view.prepared = Some(Frame {size, scale, zoom: scale, phase: 0.0});
                            view.draw_prepared(painter, repair)
                        }).unwrap().unwrap();
                        *changed += plan.damage.rects().iter()
                            .map(|r| u64::from(r.width) * u64::from(r.height)).sum::<u64>();
                        *repaired += plan.repaired_pixels;
                        *copied += plan.copied_pixels;
                        *frames += 1;
                    },
                    |(view, _, _, surface, changed, repaired, copied, frames)| serde_json::json!({
                        "canvas_bytes": surface.bytes(),
                        "render_threads": view.render_threads,
                        "mean_damage_pixels": changed / frames.max(&1),
                        "mean_repair_pixels": repaired / frames.max(&1),
                        "mean_copied_pixels": copied / frames.max(&1),
                        "note": "Three presentation targets, same scene/repair history for every policy. Native mode calls the production queued path. No compositor or game timing."
                    }),
                ));
            }
        }
    }
    render_bench::report("poe2-drag", cases);
}
