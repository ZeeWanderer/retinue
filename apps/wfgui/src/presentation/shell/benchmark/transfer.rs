use super::*;
use retinue_render::{
    painter::PixelSource,
    surface::experiment::{Transfer, Trial},
};
use rustix::time::{ClockId, clock_gettime};

#[test]
#[ignore = "independent-source partial repair and presentation-copy experiment"]
fn independent_source_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let threads = crate::app::benchmark::threads();
    let materializations =
        std::env::var("GUI_BENCH_MATERIALIZATIONS").unwrap_or_else(|_| "explicit".to_owned());
    let artwork: Vec<crate::data::foundry::Descriptor> = std::env::var_os("GUI_BENCH_ARTWORK")
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .unwrap_or_default();
    let size = (2560, 1440);
    let cpu = || {
        let time = clock_gettime(ClockId::ProcessCPUTime);
        time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1e6
    };
    let mut reports = Vec::new();
    let mut policies = vec![
        ("repair", Transfer::NoRelocate, 3, false),
        ("in-place", Transfer::InPlace, 3, false),
        ("previous-3", Transfer::Previous, 3, false),
        ("previous-regions", Transfer::PreviousRegions, 3, false),
        ("previous-cpu", Transfer::PreviousCpu, 3, false),
        (
            "previous-cpu-parallel",
            Transfer::PreviousCpuParallel,
            3,
            false,
        ),
        ("previous-2", Transfer::Previous, 2, false),
        ("snapshot-full", Transfer::SnapshotFull, 3, false),
        ("snapshot-regions", Transfer::SnapshotRegions, 3, false),
        ("offscreen-2", Transfer::Previous, 2, true),
    ];
    if let Ok(cases) = std::env::var("GUI_BENCH_CASES") {
        policies.retain(|(name, ..)| cases.split(',').any(|case| case == *name));
    }
    if std::env::var_os("GUI_BENCH_REVERSE").is_some() {
        policies.reverse();
    }
    let scales = crate::app::benchmark::scales(&[
        0.90, 0.95, 1.00, 1.05, 1.10, 1.15, 1.20, 1.25, 1.30, 1.50,
    ]);
    for scale in scales {
        let mut transfer_coverage = None;
        for &(mode, policy, count, offscreen) in &policies {
            let mut shell = Shell::new(Some(Snapshot::fixture()), scale as f32).unwrap();
            shell.foundry.benchmark_materializations(&materializations);
            // Loading pulses must not change repair coverage with executor speed.
            shell.pages.activity = json!({});
            shell.resize(size, 1.0).unwrap();
            shell.set_page(Page::Foundry);
            let mut fixture = crate::data::foundry::fixture();
            if !artwork.is_empty() {
                let asset = |index: usize| {
                    Some(crate::data::foundry::Asset {
                        id: artwork[index % artwork.len()].id.clone(),
                        ..Default::default()
                    })
                };
                for (index, card) in fixture.items.iter_mut().enumerate() {
                    card.asset = asset(index);
                    for (part, component) in card.components.iter_mut().enumerate() {
                        component.asset = asset(index + part + 1);
                    }
                }
            }
            shell.foundry.update(fixture);
            let mut prepared_images = std::collections::HashSet::new();
            if !artwork.is_empty() {
                shell.foundry.assets(artwork.clone());
                let mut cache = crate::presentation::foundry::images::cache(None);
                let requests = shell.foundry.image_requests();
                prepared_images.extend(requests.iter().map(|r| r.key));
                cache.begin_batch(&requests);
                let images = requests
                    .iter()
                    .map(|r| (r.key, cache.prepare(r).unwrap()))
                    .collect();
                shell.foundry.accept_images(images, Vec::new());
            }
            let center = shell.geometry.center;
            let viewport = shell.foundry.capture_bounds("foundry.grid").unwrap();
            let x = (center.x + center.width - shell.px(9.0)) as f64;
            let y = (center.y + viewport.y + shell.px(3.0)) as f64;
            shell.pointer_move((x, y));
            shell.pointer_button(true, Instant::now());
            let mut trial = Trial::new(size, count, policy);
            let mut presented = if offscreen {
                vec![vec![0; size.0 as usize * size.1 as usize * 4]; 3]
            } else {
                Vec::new()
            };
            let mut present_damage = retinue_render::damage::RepaintTracker::default();
            for _ in 0..3 {
                present_damage.add_buffer();
            }
            let mut samples = Vec::new();
            for frame in 0..160 {
                let process = cpu();
                let start = Instant::now();
                for event in 0..8 {
                    let phase = ((frame * 8 + event) % 400) as f64 / 400.0;
                    let fraction = if phase < 0.5 { phase } else { 1.0 - phase };
                    let position = (
                        x - f64::from(shell.px(100.0)),
                        y + fraction * f64::from(viewport.height - shell.px(40.0)),
                    );
                    std::hint::black_box(shell.cursor());
                    trial.invalidate(shell.pointer_move(position));
                    std::hint::black_box(shell.cursor());
                }
                trial.invalidate(shell.tick(Instant::now()));
                let input = Instant::now();
                shell.prepare_paint();
                let recorded = Instant::now();
                let scene = shell.paint_scene();
                let prepared = Instant::now();
                let lease = shell.render_host.begin_frame(threads);
                let slot = frame % count;
                let sample = trial
                    .paint(slot, scene, lease.threads(), |p, _| {
                        p.fill_rounded_rect(0, 0, size.0, size.1, 0, BACKGROUND);
                        shell.draw_prepared(p);
                        Ok(())
                    })
                    .unwrap()
                    .unwrap();
                let painted = Instant::now();
                let mut present_pixels = 0;
                if offscreen {
                    let index = frame % 3;
                    present_damage.invalidate(Damage::region(sample.damage.clone()));
                    let plan = present_damage.plan(index, size).unwrap();
                    let source = PixelSource::new(trial.pixels(slot), size.0, size.1).unwrap();
                    let mut painter = Painter::with_threads(
                        &mut presented[index],
                        size.0,
                        size.1,
                        lease.threads(),
                    )
                    .unwrap();
                    for r in plan.repair.rects() {
                        painter.copy_source(&source, *r, (r.x, r.y));
                    }
                    painter.finish().unwrap();
                    present_pixels = plan.repair.area();
                    present_damage.submitted(index);
                }
                if frame >= 20 {
                    let finished = Instant::now();
                    let process_ms = cpu() - process;
                    let stats = shell.foundry.paint_stats();
                    let raster = shell.foundry.raster_stats();
                    samples.push(json!({
                        "total_ms":(finished-start).as_secs_f64()*1000.0,
                        "process_cpu_ms":process_ms,
                        "prepare_ms":(prepared-start).as_secs_f64()*1000.0,
                        "input_ms":(input-start).as_secs_f64()*1000.0,
                        "record_ms":(recorded-input).as_secs_f64()*1000.0,
                        "scene_ms":(prepared-recorded).as_secs_f64()*1000.0,
                        "paint_ms":(painted-prepared).as_secs_f64()*1000.0,
                        "present_copy_ms":(finished-painted).as_secs_f64()*1000.0,
                        "plan_ms":sample.planning_ns as f64/1e6,
                        "snapshot_ms":sample.snapshot_ns as f64/1e6,
                        "copy_submit_ms":sample.copy_submit_ns as f64/1e6,
                        "paint_submit_ms":sample.paint_submit_ns as f64/1e6,
                        "finish_ms":sample.finish_ns as f64/1e6,
                        "copied_pixels":sample.copied_pixels,
                        "snapshot_pixels":sample.snapshot_pixels,
                        "repair_pixels":sample.repaired_pixels,
                        "present_pixels":present_pixels,
                        "replayed_commands":stats.replayed_commands,
                        "blits":stats.blits,
                        "fragment_blits":stats.fragment_blits,
                        "prepared_rasters":stats.prepared_rasters,
                        "prepared_fragments":stats.prepared_fragments,
                        "deferred_preparations":stats.deferred_preparations,
                        "preparation_tasks":stats.preparation_tasks,
                        "materialization_ms":stats.materialization_ns as f64 / 1e6,
                        "raster_bytes":raster.live_bytes,
                        "raster_warm_bytes":raster.warm_bytes,
                    }));
                    #[cfg(feature = "profiling")]
                    {
                        let sample = samples.last_mut().unwrap();
                        sample["foundry_profile"] = shell.foundry.benchmark_profile();
                        sample["shell_paint"] =
                            json!(shell.paint_profile.get().map(|d| d.as_secs_f64() * 1000.0));
                    }
                }
                if !artwork.is_empty() {
                    assert!(
                        shell
                            .foundry
                            .image_requests()
                            .iter()
                            .all(|r| prepared_images.contains(&r.key)),
                        "artwork workload must remain fully warmed at scale {scale}, frame {frame}"
                    );
                }
            }
            shell.pointer_button(false, Instant::now());
            if matches!(
                policy,
                Transfer::Previous
                    | Transfer::PreviousRegions
                    | Transfer::PreviousCpu
                    | Transfer::PreviousCpuParallel
            ) && count == 3
                && !offscreen
            {
                let coverage: Vec<_> = samples
                    .iter()
                    .map(|sample| {
                        (
                            sample["copied_pixels"].as_u64().unwrap(),
                            sample["repair_pixels"].as_u64().unwrap(),
                        )
                    })
                    .collect();
                if let Some(expected) = &transfer_coverage {
                    assert_eq!(&coverage, expected, "copy workload changed for {mode}");
                } else {
                    transfer_coverage = Some(coverage);
                }
            }
            let mut summary = serde_json::Map::new();
            for field in [
                "total_ms",
                "process_cpu_ms",
                "prepare_ms",
                "input_ms",
                "record_ms",
                "scene_ms",
                "paint_ms",
                "present_copy_ms",
                "plan_ms",
                "snapshot_ms",
                "copy_submit_ms",
                "paint_submit_ms",
                "finish_ms",
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
            reports.push(json!({"scale":scale,"mode":mode,"threads":threads.get(),
                "materializations":materializations,
                "framebuffer":size,"desktop_scale":1.0,
                "over_grid":true,"summary":summary,"samples":samples,
                "pixel_bytes":trial.bytes()+presented.iter().map(Vec::len).sum::<usize>(),
                "artwork_digests":artwork.iter().map(|a| &a.digest).collect::<Vec<_>>(),
                "note":"Synthetic cards, optional captured artwork reused as both main and component images. Actual shell/Foundry; rotating buffers; source copies and repairs share a native context. Offscreen includes copying into a three-buffer presentation model, not compositor time."}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
