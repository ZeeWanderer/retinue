use super::*;
use clap::Parser;
use rustix::time::{ClockId, clock_gettime};

#[test]
#[ignore = "opt-in scroll benchmark; GUI_BENCH_LIVE=1 uses cached player data"]
fn relic_scroll_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let live = std::env::var("GUI_BENCH_LIVE").is_ok_and(|v| v == "1");
    let threads = crate::benchmark::threads();
    let cpu = || {
        let time = clock_gettime(ClockId::ProcessCPUTime);
        time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1e6
    };
    let mut reports = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        let size = (2560, 1440);
        let mut args = vec!["wfgui".to_owned(), "--page".into(), "relics".into()];
        if !live {
            args.push("--fixture".into());
        }
        let args = crate::Args::parse_from(args);
        let mut shell =
            crate::shell::Shell::new(Some(crate::data::Snapshot::fixture()), scale).unwrap();
        shell.resize(size, 1.0).unwrap();
        shell.set_page(Page::Relics);
        crate::prepare_capture(&mut shell, &args).unwrap();
        let pages = &mut shell.pages;
        if !live {
            let source = pages.relics.items[0].clone();
            let rows: Vec<_> = (0..300)
                .map(|i| {
                    let mut row = source.clone();
                    row["id"] = json!(format!("relic-{i}"));
                    row["name"] = json!(format!("Lith {i:03}"));
                    row["rewards"] = json!(
                        (0..6)
                            .map(|j| source["rewards"][j % 2].clone())
                            .collect::<Vec<_>>()
                    );
                    row
                })
                .collect();
            pages.relics.replace(json!({"items": rows}), &pages.quotes);
            pages.dirty = true;
            pages.layout();
        }
        let width = pages.ui.size.0;
        let height = pages.ui.size.1;
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for mode in ["wheel", "drag"] {
            pages.relics.scroll.to(0.0, false, Instant::now());
            pages.relics.tick(Instant::now());
            pages.dirty = true;
            pages.layout();
            let track = pages.ui.bounds("list:scroll").unwrap();
            if mode == "drag" {
                pages.pointer(false, (track.x as f64 + 3.0, track.y as f64 + 3.0));
                pages.button(false, true);
                pages.tick(Instant::now());
            }
            let mut samples = Vec::new();
            for frame in 0..160 {
                let process = cpu();
                let start = Instant::now();
                if mode == "drag" {
                    for event in 0..8 {
                        let t = (frame * 8 + event) as f64 / 1280.0;
                        let travel = (track.height as f64 - 32.0).max(1.0);
                        let fraction = if t < 0.5 { t } else { 1.0 - t };
                        pages.pointer(
                            false,
                            (
                                track.x as f64 + 3.0,
                                track.y as f64 + 3.0 + fraction * travel,
                            ),
                        );
                    }
                } else {
                    pages.wheel(false, if frame < 80 { 5.0 } else { -5.0 }, false);
                }
                pages.tick(Instant::now());
                let updated = Instant::now();
                let mut painter =
                    Painter::with_threads(&mut pixels, width, height, threads).unwrap();
                pages.paint(&mut painter);
                painter.finish().unwrap();
                if frame >= 20 {
                    samples.push(json!({"update_ms":(updated-start).as_secs_f64()*1000.0,
                        "paint_ms":updated.elapsed().as_secs_f64()*1000.0,
                        "total_ms":start.elapsed().as_secs_f64()*1000.0,
                        "process_cpu_ms":cpu()-process}));
                }
            }
            pages.button(false, false);
            let mut summary = serde_json::Map::new();
            for metric in ["update_ms", "paint_ms", "total_ms", "process_cpu_ms"] {
                let mut values: Vec<_> = samples
                    .iter()
                    .map(|s| s[metric].as_f64().unwrap())
                    .collect();
                values.sort_by(f64::total_cmp);
                summary.insert(
                    metric.into(),
                    json!({"p50":values[values.len()/2],
                    "p95":values[(values.len()-1)*95/100], "max":values.last()}),
                );
            }
            reports.push(json!({"size":[width,height],"scale":scale,"mode":mode,"live":live,
                "threads":threads.get(),"items":pages.relics.visible.len(),"summary":summary,"samples":samples}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
