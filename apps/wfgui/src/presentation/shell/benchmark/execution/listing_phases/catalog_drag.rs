use super::*;
use crate::presentation::benchmark::{listing_digest, relic_drag_points};
use clap::Parser;
use std::hash::Hasher;

fn page_hash(pixels: &[u8], r: Rect) -> String {
    let mut h = rustc_hash::FxHasher::default();
    for y in r.y..r.y + r.height {
        let start = ((y * SIZE.0 + r.x) * 4) as usize;
        h.write(&pixels[start..start + r.width as usize * 4]);
    }
    format!("{:016x}", h.finish())
}

#[test]
#[ignore = "real catalog scrollbar drag through shell input, buffer repair and packet execution"]
fn shell_catalog_drag() {
    let scale: f32 = std::env::var("GUI_BENCH_SCALE")
        .unwrap_or_else(|_| "2".into())
        .parse()
        .unwrap();
    let frames: usize = std::env::var("GUI_BENCH_FRAMES").map_or(160, |s| s.parse().unwrap());
    assert!(frames > 20);
    let threads = bench::render_threads().get();
    let mut run = setup_with(
        Page::Relics,
        scale,
        HostPolicy {
            threads,
            ..HostPolicy::desktop()
        },
        Mode::Desktop,
        |_| {},
    );
    crate::app::prepare_capture(
        &mut run.shell,
        &crate::app::Args::parse_from(["wfgui", "--page", "relics"]),
    )
    .unwrap();
    let mut buffers = Buffers::new();
    for _ in 0..3 {
        buffers.frame(&mut run, |_| Damage::Full);
    }
    let pages = &run.shell.pages;
    let digest = listing_digest(pages);
    let catalog_items = pages.relics.items.len();
    let items = pages.relics.visible.len();
    let mut track = pages
        .ui
        .bounds("list:scroll")
        .expect("scrollable real listing");
    let center = run.shell.geometry.center;
    track.x += center.x;
    track.y += center.y;
    let hash_pixels = std::env::var_os("GUI_BENCH_CHECK_PIXELS").is_some();
    buffers.frame(&mut run, |shell| {
        let now = Instant::now();
        shell
            .pointer_move((track.x as f64 + 3., track.y as f64 + 3.))
            .union(shell.pointer_button(true, now))
            .union(shell.tick(now))
    });
    let mut samples = Vec::new();
    for i in 0..frames {
        let before = run.shell.pages.relic_window();
        let old_offset = run.shell.pages.relics.offset;
        let times = buffers.frame(&mut run, |shell| {
            let dirty = relic_drag_points(track, i, frames)
                .fold(Damage::None, |d, point| d.union(shell.pointer_move(point)));
            dirty.union(shell.tick(Instant::now()))
        });
        if i >= 20 {
            let after = run.shell.pages.relic_window();
            #[cfg(feature = "geometry-census")]
            run.solver.record(if before == after {
                "steady"
            } else {
                "membership"
            });
            let offset = run.shell.pages.relics.offset;
            let mut sample = json!({"frame":i, "window":{"before":[before.start,before.end],
                "after":[after.start,after.end], "offset_before":old_offset,"offset_after":offset}, "ns":times});
            if hash_pixels {
                sample["page_hash"] =
                    json!(page_hash(&buffers.pixels[(buffers.count - 1) % 3], center));
            }
            samples.push(sample);
        }
    }
    assert!(
        samples
            .iter()
            .any(|s| s["window"]["before"] != s["window"]["after"]),
        "no membership changes"
    );
    run.shell.pointer_button(false, Instant::now());
    let report = json!({"scale":scale,"threads":threads,"size":SIZE,"page_size":[center.width,center.height],
        "listing_digest":digest,"catalog_items":catalog_items,"items":items,"columns":["total_ns","input_layout_ns","prepare_packet_ns","native_ns","repair_px","present_px"], "samples":samples});
    #[cfg(feature = "geometry-census")]
    let report = {
        let mut report = report;
        run.solver.attach(&mut report, 0);
        report
    };
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
}
