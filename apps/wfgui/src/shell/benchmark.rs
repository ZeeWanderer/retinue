use super::*;
use retinue_ui::{damage::AgeRepaintTracker, painter::render_thread_budget};
use serde_json::json;

#[test]
#[ignore = "opt-in animation benchmark"]
fn rail_animation_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let threads = render_thread_budget(4);
    let mut reports = Vec::new();
    for (width, height, scale) in [(1920, 1080, 1.0), (2560, 1440, 1.0), (2560, 1440, 1.25)] {
        let mut shell = Shell::new(Some(Snapshot::fixture()), scale).unwrap();
        shell.resize((width, height), 1.0).unwrap();
        let mut buffer = vec![0_u32; width as usize * height as usize];
        let mut damage = AgeRepaintTracker::default();
        let mut samples = Vec::new();
        let origin = Instant::now();
        for cycle in 0..8 {
            let now = origin + Duration::from_secs(cycle);
            shell.activate(Control::Left, now);
            for frame in 0..=20 {
                let start = Instant::now();
                damage.invalidate(shell.tick(now + MOTION.mul_f32(frame as f32 / 20.0)));
                let laid_out = Instant::now();
                let Some(plan) = damage.plan(u8::from(cycle != 0 || frame != 0), (width, height))
                else {
                    continue;
                };
                let mut painter = Painter::with_threads(
                    bytemuck::cast_slice_mut(&mut buffer),
                    width,
                    height,
                    threads,
                )
                .unwrap();
                painter.clip_to_rect(plan.repair);
                fill(&mut painter, plan.repair, 0, BACKGROUND);
                shell.paint(&mut painter);
                painter.finish().unwrap();
                let finished = Instant::now();
                damage.submitted();
                if cycle > 1 {
                    samples.push(json!({
                        "layout_ms": (laid_out - start).as_secs_f64() * 1000.0,
                        "paint_ms": (finished - laid_out).as_secs_f64() * 1000.0,
                        "total_ms": (finished - start).as_secs_f64() * 1000.0,
                    }));
                }
            }
        }
        let mut summary = serde_json::Map::new();
        for field in ["layout_ms", "paint_ms", "total_ms"] {
            let mut values: Vec<_> = samples.iter().map(|v| v[field].as_f64().unwrap()).collect();
            values.sort_by(f64::total_cmp);
            summary.insert(
                field.into(),
                json!({
                    "p50": values[values.len() / 2],
                    "p95": values[(values.len() - 1) * 95 / 100],
                    "max": values.last(),
                }),
            );
        }
        reports.push(json!({"width": width, "height": height, "scale": scale,
            "threads": threads.get(), "summary": summary, "samples": samples}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
