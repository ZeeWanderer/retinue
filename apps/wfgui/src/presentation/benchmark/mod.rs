use super::*;
use clap::Parser;
use rustix::{time::ClockId, time::clock_gettime};

mod replay;

impl Pages {
    pub(crate) fn residency_signal(&mut self, key: &str, value: Option<&str>) {
        let signal = Signal {
            key: key.into(),
            value: value.map(str::to_owned),
        };
        if key.starts_with("rail:") {
            self.rail_signal(signal);
        } else if key.starts_with("build:") {
            self.build_signal(signal);
        } else if key.starts_with("market:") {
            self.market_signal(signal);
        } else {
            self.listing_signal(signal);
        }
        self.dirty = true;
        self.layout();
    }
}

#[test]
#[ignore = "opt-in isolated subtree motion experiment"]
fn subtree_motion_benchmark() {
    use retinue_render::painter::RasterImage;
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let mut reports = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        for cached in [false, true] {
            let mut pages = Pages::new(Default::default()).unwrap();
            pages.fixture();
            let source = serde_json::to_value(&pages.mastery.items[0]).unwrap();
            let rows: Vec<_> = (0..100)
                .map(|i| {
                    let mut row = source.clone();
                    row["id"] = json!(format!("mastery-{i}"));
                    row["name"] = json!(format!("Frame {i:03}"));
                    row
                })
                .collect();
            pages
                .mastery
                .replace_fixture(json!({"items":rows}), &pages.quotes);
            pages.set_page(Page::Mastery);
            pages.configure((1800, 1360), (400, 1360), scale);
            let grid = pages.ui.bounds("page.grid").unwrap();
            let size = pages.ui.size;
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut layer = None;
            let mut samples = Vec::new();
            let mut preparation_ms = 0.0;
            let mut max_error = 0;
            for frame in 0..80 {
                if frame == 40 {
                    pages.mastery.items[0].name = "Updated frame".into();
                    pages.dirty = true;
                    pages.layout();
                    layer = None;
                }
                let start = Instant::now();
                if cached && layer.is_none() {
                    layer = Some(
                        RasterImage::render(grid.width, grid.height, |p| {
                            p.fill_rounded_rect(
                                0,
                                0,
                                grid.width,
                                grid.height,
                                0,
                                crate::presentation::style::PANEL,
                            );
                            p.translated((-(grid.x as i32), -(grid.y as i32)), |p| pages.paint(p));
                        })
                        .unwrap(),
                    );
                    preparation_ms += start.elapsed().as_secs_f64() * 1000.0;
                }
                let dy = (frame % 20) * pages.ui.px(3.0);
                let mut p = Painter::with_threads(
                    &mut pixels,
                    size.0,
                    size.1,
                    crate::app::benchmark::threads(),
                )
                .unwrap();
                p.clear();
                if let Some(layer) = &layer {
                    p.draw_image(layer, grid.x, grid.y + dy, grid.width, grid.height);
                } else {
                    paint_moving_grid(&pages, &mut p, grid, dy);
                }
                p.finish().unwrap();
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
                if cached && [0, 19, 40, 59].contains(&frame) {
                    let mut expected = vec![0; pixels.len()];
                    let mut p = Painter::with_threads(
                        &mut expected,
                        size.0,
                        size.1,
                        crate::app::benchmark::threads(),
                    )
                    .unwrap();
                    paint_moving_grid(&pages, &mut p, grid, dy);
                    p.finish().unwrap();
                    max_error = max_error.max(
                        expected
                            .iter()
                            .zip(&pixels)
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap(),
                    );
                    assert!(max_error <= 2, "subtree pixels differ: {max_error}");
                }
            }
            let total_ms = samples.iter().sum::<f64>();
            let first_12_ms = samples[..12].iter().sum::<f64>();
            let first_20_ms = samples[..20].iter().sum::<f64>();
            samples.sort_by(f64::total_cmp);
            reports.push(json!({"scale":scale,"cached":cached,"frames":samples.len(),"total_ms":total_ms,
                "p50_ms":samples[samples.len()/2],"p95_ms":samples[(samples.len()-1)*95/100],
                "preparation_ms":preparation_ms,"first_12_ms":first_12_ms,"first_20_ms":first_20_ms,
                "layer_bytes":layer.as_ref().map_or(0,RasterImage::byte_len),"max_pixel_error":max_error}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}

fn paint_moving_grid(pages: &Pages, p: &mut Painter<'_>, grid: Rect, dy: u32) {
    p.translated((0, dy as i32), |p| {
        p.viewport(grid, (0, 0), |p| {
            p.fill_rounded_rect(
                grid.x,
                grid.y,
                grid.width,
                grid.height,
                0,
                crate::presentation::style::PANEL,
            );
            pages.paint(p);
        });
    });
}

#[test]
#[ignore = "opt-in adaptive layer motion and invalidation benchmark"]
fn adaptive_motion_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let mut reports = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        let order = if std::env::var_os("GUI_BENCH_ADAPTIVE_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for cached in order {
            let mut pages = Pages::new(Default::default()).unwrap();
            pages.fixture();
            let source = serde_json::to_value(&pages.mastery.items[0]).unwrap();
            let rows: Vec<_> = (0..100)
                .map(|i| {
                    let mut row = source.clone();
                    row["id"] = json!(format!("mastery-{i}"));
                    row["name"] = json!(format!("Frame {i:03}"));
                    row
                })
                .collect();
            pages
                .mastery
                .replace_fixture(json!({"items": rows}), &pages.quotes);
            pages.set_page(Page::Mastery);
            pages.configure((1800, 1360), (400, 1360), scale);
            pages.ui.set_layer_caching(cached);
            let size = pages.ui.size;
            let grid = pages.ui.bounds("page.grid").unwrap();
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut samples = Vec::new();
            let (mut promotions, mut demotions, mut layer_bytes, mut preparation_ns) = (0, 0, 0, 0);
            let mut max_error = 0;
            for frame in 0..100 {
                let start = Instant::now();
                if frame == 40 {
                    pages.mastery.items[0].name = "Updated frame".into();
                    pages.dirty = true;
                    pages.layout();
                }
                let mut p = Painter::with_threads(
                    &mut pixels,
                    size.0,
                    size.1,
                    crate::app::benchmark::threads(),
                )
                .unwrap();
                p.clear();
                paint_moving_grid(&pages, &mut p, grid, (frame % 20) * pages.ui.px(3.0));
                p.finish().unwrap();
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
                let stats = pages.ui.paint_stats();
                promotions += stats.promoted_layers;
                demotions += stats.demoted_layers;
                preparation_ns += stats.layer_prepare_ns;
                layer_bytes = layer_bytes.max(stats.layer_bytes);
                if [9, 19, 40, 49, 59].contains(&frame) {
                    let mut expected = vec![0; pixels.len()];
                    pages.ui.with_full_replay(|| {
                        let mut p = Painter::with_threads(
                            &mut expected,
                            size.0,
                            size.1,
                            crate::app::benchmark::threads(),
                        )
                        .unwrap();
                        p.clear();
                        paint_moving_grid(&pages, &mut p, grid, (frame % 20) * pages.ui.px(3.0));
                        p.finish().unwrap();
                    });
                    max_error = max_error.max(
                        expected
                            .iter()
                            .zip(&pixels)
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap(),
                    );
                    assert!(
                        max_error <= 2,
                        "layer error={max_error} scale={scale} frame={frame}"
                    );
                }
            }
            let total_ms: f64 = samples.iter().sum();
            let first_12_ms: f64 = samples[..12].iter().sum();
            let first_20_ms: f64 = samples[..20].iter().sum();
            let raw = samples.clone();
            samples.sort_by(f64::total_cmp);
            reports.push(json!({"scale":scale,"adaptive":cached,"total_ms":total_ms,
                "first_12_ms":first_12_ms,"first_20_ms":first_20_ms,"p50_ms":samples[50],"p95_ms":samples[94],"max_ms":samples[99],
                "promotions":promotions,"demotions":demotions,"layer_bytes":layer_bytes,
                "preparation_ms":preparation_ns as f64 / 1e6,"max_pixel_error":max_error,"samples":raw}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}

#[test]
#[ignore = "opt-in scroll benchmark; GUI_BENCH_LIVE=1 uses cached player data"]
fn relic_scroll_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let live = std::env::var("GUI_BENCH_LIVE").is_ok_and(|v| v == "1");
    let threads = crate::app::benchmark::threads();
    let selected_scale = std::env::var("GUI_BENCH_SCALE")
        .ok()
        .map(|v| v.parse::<f32>().unwrap());
    assert!(
        selected_scale.is_none_or(|scale| [1.0, 1.25, 2.0].contains(&scale)),
        "GUI_BENCH_SCALE must be 1, 1.25 or 2"
    );
    let frames = std::env::var("GUI_BENCH_FRAMES").map_or(160, |v| v.parse::<usize>().unwrap());
    assert!(frames > 20);
    let cpu = || {
        let time = clock_gettime(ClockId::ProcessCPUTime);
        time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1e6
    };
    let mut reports = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        if selected_scale.is_some_and(|selected| selected != scale) {
            continue;
        }
        let size = (2560, 1440);
        let mut args = vec!["wfgui".to_owned(), "--page".into(), "relics".into()];
        if !live {
            args.push("--fixture".into());
        }
        let args = crate::app::Args::parse_from(args);
        let mut shell = crate::presentation::shell::Shell::new(
            Some(crate::data::storage::Snapshot::fixture()),
            scale,
        )
        .unwrap();
        shell.resize(size, 1.0).unwrap();
        shell.set_page(Page::Relics);
        crate::app::prepare_capture(&mut shell, &args).unwrap();
        let pages = &mut shell.pages;
        if !live {
            let source = serde_json::to_value(&pages.relics.items[0]).unwrap();
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
            pages
                .relics
                .replace_fixture(json!({"items": rows}), &pages.quotes);
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
            for frame in 0..frames {
                let process = cpu();
                let start = Instant::now();
                if mode == "drag" {
                    for event in 0..8 {
                        let t = (frame * 8 + event) as f64 / (frames * 8) as f64;
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
                    pages.wheel(false, if frame < frames / 2 { 5.0 } else { -5.0 }, false);
                }
                pages.tick(Instant::now());
                let updated = Instant::now();
                #[cfg(feature = "profiling")]
                let (view_stats, profile, scene, scene_stats) = pages.ui.view_profile();
                let mut painter =
                    Painter::with_threads(&mut pixels, width, height, threads).unwrap();
                pages.paint(&mut painter);
                painter.finish().unwrap();
                if frame >= 20 {
                    let mut sample = json!({"update_ms":(updated-start).as_secs_f64()*1000.0,
                        "paint_ms":updated.elapsed().as_secs_f64()*1000.0,
                        "total_ms":start.elapsed().as_secs_f64()*1000.0,
                        "process_cpu_ms":cpu()-process});
                    #[cfg(feature = "profiling")]
                    {
                        let (binding, prepare, draw) = pages.ui.paint_profile();
                        let render = pages.ui.paint_stats();
                        sample["view"] = json!({"bodies":view_stats.bodies,
                            "mounted":view_stats.mounted,"unmounted":view_stats.unmounted,
                            "projected":view_stats.projected,
                            "evaluation_ms":profile.evaluation.as_secs_f64()*1000.0,
                            "lowering_ms":profile.lowering.as_secs_f64()*1000.0,
                            "scene_ms":profile.scene.as_secs_f64()*1000.0,
                            "input_ms":profile.input.as_secs_f64()*1000.0,
                            "control_input_ms":pages.ui.input_profile.as_secs_f64()*1000.0,
                            "listing_sync_ms":pages.listing_profile[0].as_secs_f64()*1000.0,
                            "listing_project_ms":pages.listing_profile[1].as_secs_f64()*1000.0,
                            "listing_bind_ms":pages.listing_profile[2].as_secs_f64()*1000.0,
                            "reconcile_ms":scene.reconcile.as_secs_f64()*1000.0,
                            "layout_ms":scene.layout.as_secs_f64()*1000.0,
                            "projection_ms":scene.projection.as_secs_f64()*1000.0,
                            "geometry_ms":scene.geometry.as_secs_f64()*1000.0,
                            "frame_ms":scene.frame.as_secs_f64()*1000.0,
                            "nodes":scene_stats.nodes,"recordings":scene_stats.recordings,
                            "layout_computations":scene_stats.layout_computations,
                            "paint_binding_ms":binding.as_secs_f64()*1000.0,
                            "prepare_ms":prepare.as_secs_f64()*1000.0,
                            "submit_ms":draw.as_secs_f64()*1000.0,
                            "inspected_instances":render.inspected_instances,
                            "changed_programs":render.changed_programs,
                            "inspected_groups":render.inspected_groups,
                            "inspected_regions":render.inspected_regions,
                            "promoted_layers":render.promoted_layers,
                            "layer_prepare_ms":render.layer_prepare_ns as f64 / 1e6,
                            "prepared_rasters":render.prepared_rasters,
                            "replayed_commands":render.replayed_commands,
                            "layer_bytes":render.layer_bytes});
                    }
                    samples.push(sample.take());
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
