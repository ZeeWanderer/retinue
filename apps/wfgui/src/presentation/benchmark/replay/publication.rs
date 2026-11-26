use super::*;

#[test]
#[ignore = "publication and frozen-packet replay floor on identical Relic edits and damage"]
fn publication_trial() {
    let trace = trace_samples(1.25, 36);
    let size = trace[0].snapshot.frame().size();
    let full = Rect {
        width: size.0,
        height: size.1,
        ..Rect::default()
    };
    let policy = std::env::var("RETINUE_BENCH_PUBLICATION").unwrap_or_else(|_| "current".into());
    let mut samples = Vec::new();
    for verify in [true, false] {
        let mut renderer = Policy::Layers.renderer();
        let mut oracle = Policy::Raw.renderer();
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut reference = pixels.clone();
        for (i, sample) in trace.iter().enumerate() {
            let frame = sample.snapshot.frame();
            let repair = sample.damage.regions(size);
            if repair.is_empty() {
                continue;
            }
            let start = Instant::now();
            renderer.prepare_for(frame, RasterMode::Explicit, &repair);
            let packet = renderer
                .freeze(frame, &repair, |_, _| unreachable!())
                .unwrap();
            let recorded = Instant::now();
            let mut p = Painter::with_threads(
                &mut pixels,
                size.0,
                size.1,
                crate::app::benchmark::threads(),
            )
            .unwrap();
            p.repair(&repair);
            p.clear();
            p.fill_rounded_rect(0, 0, size.0, size.1, 0, Color::rgb(0x121419).bgra());
            let draw = packet.draw(&mut p);
            p.finish().unwrap();
            let finished = Instant::now();
            if verify {
                oracle.prepare(frame, RasterMode::Replay);
                let mut p = Painter::with_threads(
                    &mut reference,
                    size.0,
                    size.1,
                    crate::app::benchmark::threads(),
                )
                .unwrap();
                oracle.paint(frame, &mut p, full, Color::rgb(0x121419));
                p.finish().unwrap();
                let cached = matches!(
                    policy.as_str(),
                    "current" | "edits-fragments" | "edits-items"
                );
                let glows = if cached { glow_regions(frame) } else { vec![] };
                for (byte, (a, b)) in pixels.iter().zip(&reference).enumerate() {
                    let error = a.abs_diff(*b);
                    if error <= u8::from(cached) {
                        continue;
                    }
                    let x = byte / 4 % size.0 as usize;
                    let y = byte / 4 / size.0 as usize;
                    assert!(
                        error <= 3 && glows.iter().any(|r| r.contains(x as f64, y as f64)),
                        "{policy}: full-replay error {error} at {x},{y} in frame {i}"
                    );
                }
            } else {
                samples.push(json!({"index":i, "repair_pixels":repair.area(),
                    "repair":repair.rects().iter().map(|r| (r.x,r.y,r.width,r.height)).collect::<Vec<_>>(),
                    "prepare_ns":(recorded-start).as_nanos() as u64,
                    "replay_floor_ns":(finished-recorded).as_nanos() as u64,
                    "backend_ns":(finished-start).as_nanos() as u64,
                    "fragment_blits":draw.fragment_blits,"raster_blits":draw.blits,
                    "replayed":draw.replayed_commands, "layer_bytes":renderer.stats.layer_bytes,
                    "inspected":renderer.stats.inspected_instances, "instances":frame.instances().len()}));
            }
        }
    }
    std::fs::write(
        std::env::var("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&json!({"policy":policy,"size":size,"frames":samples})).unwrap(),
    )
    .unwrap();
}
