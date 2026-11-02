use super::*;
use rustix::{time::ClockId, time::clock_gettime};

#[test]
#[ignore = "opt-in scrollbar input and rendering benchmark"]
fn foundry_drag_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let threads = crate::app::benchmark::threads();
    let cpu = || {
        let time = clock_gettime(ClockId::ProcessCPUTime);
        time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1e6
    };
    let mut reports = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        for over_grid in [false, true] {
            let mut shell = Shell::new(Some(Snapshot::fixture()), scale).unwrap();
            shell.resize((2560, 1440), 1.0).unwrap();
            shell.set_page(Page::Foundry);
            shell.foundry.update(crate::data::foundry::fixture());
            let center = shell.geometry.center;
            let viewport = shell.foundry.capture_bounds("foundry.grid").unwrap();
            let x = (center.x + center.width - shell.px(9.0)) as f64;
            let y = (center.y + viewport.y + shell.px(3.0)) as f64;
            shell.pointer_move((x, y));
            shell.pointer_button(true, Instant::now());
            let mut pixels = vec![0; 2560 * 1440 * 4];
            let mut damage = AgeRepaintTracker::default();
            damage.invalidate(Damage::Full);
            let mut samples = Vec::new();
            for frame in 0..160 {
                let process = cpu();
                let start = Instant::now();
                for event in 0..8 {
                    let phase = ((frame * 8 + event) % 400) as f64 / 400.0;
                    let fraction = if phase < 0.5 { phase } else { 1.0 - phase };
                    let position = (
                        if over_grid {
                            x - f64::from(shell.px(100.0))
                        } else {
                            x
                        },
                        y + fraction * f64::from(viewport.height - shell.px(40.0)),
                    );
                    // Native CursorMoved checks the cursor on both sides of dispatch.
                    std::hint::black_box(shell.cursor());
                    damage.invalidate(shell.pointer_move(position));
                    std::hint::black_box(shell.cursor());
                }
                damage.invalidate(shell.tick(Instant::now()));
                let input_end = Instant::now();
                let Some(plan) = damage.plan(u8::from(frame != 0), shell.size) else {
                    continue;
                };
                let native_frame = shell.render_host.begin_frame(threads);
                let mut painter =
                    Painter::with_threads(&mut pixels, 2560, 1440, native_frame.threads()).unwrap();
                painter.repair(&plan.repair);
                painter.fill_rounded_rect(0, 0, 2560, 1440, 0, BACKGROUND);
                shell.paint(&mut painter);
                let submitted = Instant::now();
                painter.finish().unwrap();
                drop(native_frame);
                let finished = Instant::now();
                damage.submitted();
                if frame >= 20 {
                    samples.push(json!({
                        "input_ms": (input_end - start).as_secs_f64() * 1000.0,
                        "submit_ms": (submitted - input_end).as_secs_f64() * 1000.0,
                        "finish_ms": (finished - submitted).as_secs_f64() * 1000.0,
                        "total_ms": (finished - start).as_secs_f64() * 1000.0,
                        "process_cpu_ms": cpu() - process,
                        "repair_pixels": plan.repair.area(),
                        "repair_regions": plan.repair.rects().len(),
                    }));
                }
            }
            shell.pointer_button(false, Instant::now());
            let mut summary = serde_json::Map::new();
            for field in [
                "input_ms",
                "submit_ms",
                "finish_ms",
                "total_ms",
                "process_cpu_ms",
            ] {
                let mut values: Vec<_> =
                    samples.iter().map(|s| s[field].as_f64().unwrap()).collect();
                values.sort_by(f64::total_cmp);
                summary.insert(
                    field.into(),
                    json!({"p50":values[values.len()/2],
                    "p95":values[(values.len()-1)*95/100],"max":values.last()}),
                );
            }
            reports.push(
                json!({"scale":scale,"over_grid":over_grid,"events_per_frame":8,
                "threads":threads.get(),"summary":summary,"samples":samples}),
            );
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
