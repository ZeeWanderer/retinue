use super::*;
use retinue_render::damage::AgeRepaintTracker;
use serde_json::json;
mod drag;
mod residency;

#[test]
#[ignore = "opt-in live daemon payload allocation and wire-size audit"]
fn client_payload_benchmark() {
    let output = std::path::PathBuf::from(std::env::var("GUI_BENCH_OUTPUT").unwrap());
    let directory = output.parent().unwrap();
    std::fs::create_dir_all(directory).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut channel = runtime.block_on(crate::domain::session::connect()).unwrap();
    let used = || {
        crate::app::diagnostics::glibc_heap()["in_use_bytes"]
            .as_u64()
            .unwrap()
    };
    let mut reports = Vec::new();
    for (name, request) in [
        ("foundry", json!({"op":"foundry_view"})),
        ("inventory", json!({"op":"inventory_view"})),
        ("mastery", json!({"op":"mastery_view"})),
        (
            "relics",
            json!({"op":"relic_planner","era":"all","only_owned":false,"fetch_prices":false,"limit":"all"}),
        ),
        ("equipment", json!({"op":"build_equipment"})),
    ] {
        let before = used();
        let packet = runtime.block_on(residency_packet(&mut channel, request));
        let wire_bytes = match &packet {
            retinue_client::transport::Packet::Data(reply) => reply.payload.len(),
            _ => 0,
        };
        fn measured<
            T: retinue_protocol::prost::Message
                + retinue_protocol::prost::Name
                + Default
                + serde::Serialize,
        >(
            packet: retinue_client::transport::Packet,
            before: u64,
        ) -> (serde_json::Value, u64) {
            let value: T = packet.decode().unwrap();
            let retained = crate::app::diagnostics::glibc_heap()["in_use_bytes"]
                .as_u64()
                .unwrap()
                .saturating_sub(before);
            (serde_json::to_value(value).unwrap(), retained)
        }
        let (reply, retained) = match name {
            "foundry" => measured::<retinue_protocol::warframe::FoundrySnapshot>(packet, before),
            "equipment" => measured::<retinue_protocol::warframe::Equipment>(packet, before),
            _ => measured::<retinue_protocol::warframe::ListingSnapshot>(packet, before),
        };
        let bytes = serde_json::to_vec(&reply).unwrap();
        std::fs::write(directory.join(format!("payload-{name}.json")), &bytes).unwrap();
        let fields: Vec<_> = reply
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                json!({"field":key,"wire_bytes":serde_json::to_vec(value).unwrap().len(),
                "items":value.as_array().map(Vec::len)})
            })
            .collect();
        reports.push(json!({"payload":name,"wire_bytes":wire_bytes,"json_bytes":bytes.len(),"retained_bytes":retained,"fields":fields}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}

#[test]
#[ignore = "opt-in repeated GUI interaction and allocation trace"]
fn interaction_residency_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let cycles = std::env::var("GUI_BENCH_CYCLES")
        .map(|n| n.parse::<usize>().unwrap())
        .unwrap_or(4);
    assert!((1..=20).contains(&cycles));
    let live = std::env::var("GUI_BENCH_LIVE").is_ok_and(|v| v == "1");
    let mut run = residency::Residency::new(live, &output);
    for cycle in 0..cycles {
        if cycle > 0 {
            run.refresh();
        }
        run.tour(cycle);
    }
    run.finish();
}

#[test]
#[ignore = "opt-in page residency benchmark"]
fn page_residency_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let mut shell = Shell::new(Some(Snapshot::fixture()), 1.25).unwrap();
    shell.resize((2560, 1440), 1.0).unwrap();
    shell.foundry.update(crate::data::foundry::fixture());
    shell.pages.fixture();
    let live = std::env::var("GUI_BENCH_LIVE").is_ok_and(|value| value == "1");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut channel = live.then(|| runtime.block_on(crate::domain::session::connect()).unwrap());
    if let Some(channel) = &mut channel {
        shell.foundry.update(
            runtime
                .block_on(crate::domain::foundry::snapshot(channel))
                .unwrap(),
        );
        for (key, body) in [
            ("inventory", json!({"op":"inventory_view"})),
            ("mastery", json!({"op":"mastery_view"})),
            (
                "relics",
                json!({"op":"relic_planner","era":"all","only_owned":false,"fetch_prices":false,"limit":"all"}),
            ),
            ("activity", json!({"op":"activity_view"})),
            ("equipment", json!({"op":"build_equipment"})),
        ] {
            let reply = runtime.block_on(residency_packet(channel, body.clone()));
            shell.pages.capture_native_reply(
                crate::domain::session::Request {
                    key: key.into(),
                    body,
                },
                crate::domain::session::Reply::decode(key, reply),
            );
        }
    }
    let mut cache = crate::presentation::foundry::images::cache(
        live.then(crate::domain::storage::native_cache_root),
    );
    let mut pixels = vec![0; 2560 * 1440 * 4];
    let mut reports = Vec::new();
    let cycles = std::env::var("GUI_BENCH_CYCLES")
        .map(|n| n.parse::<usize>().unwrap())
        .unwrap_or(1);
    assert!((1..=20).contains(&cycles));
    for (index, page) in [
        Page::Foundry,
        Page::Relics,
        Page::Mastery,
        Page::Inventory,
        Page::Builds,
        Page::Settings,
        Page::Foundry,
        Page::Settings,
    ]
    .into_iter()
    .cycle()
    .take(8 * cycles)
    .enumerate()
    {
        let start = Instant::now();
        shell.set_page(page);
        if let Some(channel) = &mut channel {
            let mut assets = shell.foundry.desired_assets();
            assets.extend(shell.pages.assets());
            for batch in assets.chunks(32) {
                let reply = runtime.block_on(residency_request(
                    channel,
                    json!({"op":"asset_resolve","assets":batch}),
                ));
                let descriptors: Vec<_> = reply["assets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(crate::domain::foundry::descriptor)
                    .collect();
                shell.foundry.assets(descriptors.clone());
                shell.pages.descriptors(descriptors);
            }
        }
        let mut requests = shell.foundry.image_requests();
        requests.extend(shell.pages.image_requests());
        cache.begin_batch(&requests);
        let images: Vec<_> = requests
            .iter()
            .map(|request| (request.key, cache.prepare(request).unwrap()))
            .collect();
        shell.pages.images(&images);
        shell.foundry.accept_images(images, Vec::new());
        shell.foundry.cache_stats = cache.stats();
        let frame = shell
            .render_host
            .begin_frame(crate::app::benchmark::threads());
        let mut painter = Painter::with_threads(&mut pixels, 2560, 1440, frame.threads()).unwrap();
        shell.paint(&mut painter);
        painter.finish().unwrap();
        drop(frame);
        let memory: std::collections::BTreeMap<_, _> =
            std::fs::read_to_string("/proc/self/smaps_rollup")
                .unwrap()
                .lines()
                .skip(1)
                .filter_map(|line| {
                    let mut fields = line.split_whitespace();
                    Some((
                        fields.next()?.trim_end_matches(':').to_owned(),
                        fields.next()?.parse::<u64>().ok()? * 1024,
                    ))
                })
                .collect();
        let stats = cache.stats();
        reports.push(json!({"page":format!("{page:?}"),"cycle":index/8,"switch_ms":start.elapsed().as_secs_f64()*1000.0,
            "process":memory,"rendering_bytes":shell.image_memory().0,
            "foundry_bytes":shell.foundry.memory().0,"page_rasters":shell.pages.ui.raster_memory().0,
            "rail_rasters":shell.pages.rail.raster_memory().0,
            "source_bytes":stats.0.live_bytes,"thumbnail_bytes":stats.1.live_bytes,
            "thumbnail_active":stats.1.retained_bytes,
            "native_pixels":retinue_render::painter::image_memory().pixel_bytes,
            "framebuffer_bytes":pixels.len(),
            "glibc_heap":crate::app::diagnostics::glibc_heap(),
            "blend2d":crate::app::diagnostics::blend2d(),
            "ram_hits":cache.work.ram_hits,"resamples":cache.work.resamples,"disk_hits":cache.work.disk_hits}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}

async fn residency_request(
    channel: &mut retinue_client::transport::Channel,
    body: serde_json::Value,
) -> serde_json::Value {
    crate::domain::session::response(residency_packet(channel, body).await.json().unwrap()).unwrap()
}

async fn residency_packet(
    channel: &mut retinue_client::transport::Channel,
    mut body: serde_json::Value,
) -> retinue_client::transport::Packet {
    body["id"] = json!(80);
    let reply = tokio::time::timeout(Duration::from_secs(30), async {
        channel.send(&body).await.unwrap();
        loop {
            let reply = channel.receive_packet().await.unwrap();
            if matches!(&reply, retinue_client::transport::Packet::Json(v) if v.get("event").is_some()) {
                continue;
            }
            assert_eq!(reply.id(), body["id"].as_u64(), "unexpected benchmark reply");
            break reply;
        }
    })
    .await
    .unwrap();
    reply
}

#[test]
#[ignore = "opt-in animation benchmark"]
fn rail_animation_benchmark() {
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let threads = crate::app::benchmark::threads();
    let bounding = std::env::var("GUI_BENCH_BOUNDING_DAMAGE").is_ok_and(|value| value == "1");
    let mut reports = Vec::new();
    for (width, height, scale) in [
        (1920, 1080, 1.0),
        (2560, 1440, 1.0),
        (2560, 1440, 1.25),
        (2560, 1440, 2.0),
    ] {
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
                #[cfg(feature = "profiling")]
                let (shell_view_profile, shell_scene_profile, shell_scene_stats, shell_view_stats) = {
                    let tree = shell.composition.tree.borrow();
                    (
                        tree.profile,
                        tree.scene().profile,
                        tree.scene().stats,
                        tree.stats,
                    )
                };
                let Some(plan) = damage.plan(u8::from(cycle != 0 || frame != 0), (width, height))
                else {
                    continue;
                };
                let repair = if bounding {
                    plan.repair.bounds().into()
                } else {
                    plan.repair
                };
                let native_frame = shell.render_host.begin_frame(threads);
                let mut painter = Painter::with_threads(
                    bytemuck::cast_slice_mut(&mut buffer),
                    width,
                    height,
                    native_frame.threads(),
                )
                .unwrap();
                painter.repair(&repair);
                let r = repair.bounds();
                painter.fill_rounded_rect(r.x, r.y, r.width, r.height, 0, BACKGROUND);
                shell.paint(&mut painter);
                painter.finish().unwrap();
                drop(native_frame);
                let finished = Instant::now();
                damage.submitted();
                if cycle > 1 {
                    let mut sample = json!({
                        "layout_ms": (laid_out - start).as_secs_f64() * 1000.0,
                        "paint_ms": (finished - laid_out).as_secs_f64() * 1000.0,
                        "total_ms": (finished - start).as_secs_f64() * 1000.0,
                        "repair_pixels": repair.area(),
                        "repair_regions": repair.rects().len(),
                    });
                    #[cfg(feature = "profiling")]
                    {
                        sample["reflow_ms"] =
                            json!(shell.reflow_profile.map(|d| d.as_secs_f64() * 1000.0));
                        sample["shell_paint_ms"] =
                            json!(shell.paint_profile.get().map(|d| d.as_secs_f64() * 1000.0));
                        sample["tick_ms"] =
                            json!(shell.tick_profile.map(|d| d.as_secs_f64() * 1000.0));
                        sample["shell_scene_ms"] = json!(
                            [
                                shell_scene_profile.reconcile,
                                shell_scene_profile.layout,
                                shell_scene_profile.projection,
                                shell_scene_profile.geometry,
                                shell_scene_profile.frame,
                            ]
                            .map(|d| d.as_secs_f64() * 1000.0)
                        );
                        sample["shell_stats"] = json!({
                            "nodes": shell_scene_stats.nodes, "layouts": shell_scene_stats.layout_computations,
                            "recordings": shell_scene_stats.recordings,
                            "bodies": shell_view_stats.bodies,
                        });
                        sample["shell_view_ms"] = json!(
                            [
                                shell_view_profile.evaluation,
                                shell_view_profile.lowering,
                                shell_view_profile.scene,
                                shell_view_profile.input,
                            ]
                            .map(|d| d.as_secs_f64() * 1000.0)
                        );
                    }
                    samples.push(sample.take());
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
            "threads": threads.get(), "bounding_damage":bounding, "summary": summary, "samples": samples}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
