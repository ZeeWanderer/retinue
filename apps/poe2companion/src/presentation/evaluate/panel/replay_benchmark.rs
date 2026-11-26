use super::*;
use crate::render_bench as bench;
use memmap2::MmapMut;
use retinue_render::{DrawPacket, PaintStats};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    num::NonZeroU32,
    os::unix::fs::MetadataExt,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

const SIZE: (u32, u32) = (2560, 1440);

fn freeze(target: &mut [u8], threads: NonZeroU32) -> DrawPacket {
    let data: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/evaluate-reference.json"
    ))
    .unwrap();
    let mut model = EvaluateModel::new("Forbidden Rites".into());
    model.evaluated(
        0,
        serde_json::from_value(data["evaluation"].clone()).unwrap(),
    );
    let mut view = EvaluatePanel::new().unwrap();
    view.preview_time = Some(1791885600);
    view.render_budget = threads;
    let interaction = Interaction::default();
    let frame = || Frame {
        size: SIZE,
        scale: 1.,
        zoom: 1.,
        phase: 0.,
    };
    let repair = Damage::Full.regions(SIZE);
    for _ in 0..6 {
        view.prepare(frame(), &model, &interaction).unwrap();
        view.paint_prepared(target, &repair).unwrap();
    }
    view.prepare(frame(), &model, &interaction).unwrap();
    let (send, recv) = mpsc::channel();
    let mut job = view
        .prepare_packet(
            &repair,
            Arc::new(move || {
                let _ = send.send(());
            }),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(packet) = job.poll() {
            return packet.unwrap();
        }
        recv.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

fn replay(
    pixels: &mut [u8],
    threads: NonZeroU32,
    packet: &DrawPacket,
    case: &str,
) -> (PaintStats, u64) {
    let mut painter = Painter::with_threads(pixels, SIZE.0, SIZE.1, threads).unwrap();
    if case != "panel" {
        painter.clear();
    }
    let stats = if case == "clear" {
        PaintStats::default()
    } else {
        packet.draw(&mut painter)
    };
    let start = Instant::now();
    painter.finish().unwrap();
    (stats, start.elapsed().as_nanos() as u64)
}

#[test]
#[ignore = "make bench BENCH_SUITE=poe2-frozen-replay"]
fn frozen_replay_benchmark() {
    let len = u64::from(SIZE.0) * u64::from(SIZE.1) * 4;
    // A paired run can lend both processes the same disposable backing pages.
    let backing = if let Some(path) = std::env::var_os("RETINUE_BENCH_SHM_FILE") {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap()
    } else {
        let f = tempfile::tempfile().unwrap();
        f.set_len(len).unwrap();
        f
    };
    backing.try_lock().expect("benchmark target already in use");
    let metadata = backing.metadata().unwrap();
    assert!(
        metadata.is_file() && metadata.nlink() == 0,
        "expected an anonymous target"
    );
    assert_eq!(metadata.len(), len);
    // SAFETY: the benchmark owns the temporary file, or exclusively leases the
    // runner's fixed-size disposable file. No concurrent writer or truncation.
    let mut pixels = unsafe { MmapMut::map_mut(&backing) }.unwrap();
    let threads = bench::render_threads();
    let packet = freeze(&mut pixels, threads);
    let mut order = ["clear", "panel", "combined"];
    let run: usize = std::env::var("RETINUE_BENCH_RUN_INDEX")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    order.rotate_left(run % 3);
    if run % 2 == 1 {
        order.reverse();
    }
    let mut cases = vec![];
    let mut panel_digest = None;
    for case in order.into_iter().filter(|c| bench::selected(c)) {
        pixels.fill(if case == "panel" { 0 } else { 0x55 });
        let (stats, _) = replay(&mut pixels, threads, &packet, case);
        let digest = format!("{:x}", Sha256::digest(&*pixels));
        assert_eq!(pixels.iter().any(|&b| b != 0), case != "clear");
        if case != "clear" {
            assert_eq!(panel_digest.get_or_insert_with(|| digest.clone()), &digest);
        }
        const WARMUP: usize = 5;
        let mut row = bench::measure_with_warmup(
            case,
            SIZE,
            1.,
            WARMUP,
            || 0u64,
            |finish, index| {
                let (_, elapsed) = replay(&mut pixels, threads, &packet, case);
                if index > WARMUP {
                    *finish += elapsed;
                }
            },
            |finish| {
                json!({
                    "pixel_sha256": digest, "finish_total_ns": finish,
                    "commands": stats.replayed_commands, "culled": stats.culled_commands,
                    "blits": stats.blits, "fragment_blits": stats.fragment_blits
                })
            },
        );
        row["actual_threads"] = json!(threads.get());
        cases.push(row);
    }
    assert!(!cases.is_empty());
    bench::report_with_scope(
        "poe2-frozen-replay",
        cases,
        "A warmed Evaluate packet on a mapped 1440p target. Clear alone, panel replay without clearing between samples, or clear plus panel. Hash is one frame from transparent. No layout, publication, compositor, game, or network in the timed loop.",
    );
}
