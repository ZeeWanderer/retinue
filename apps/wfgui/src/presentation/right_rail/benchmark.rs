use super::*;
use retinue_render::{HostPolicy, RenderHost};
use retinue_ui::timeline::FrameTime;
use std::sync::{Arc, mpsc};

#[allow(dead_code)]
#[path = "../../../../../tools/benchmarks/support.rs"]
mod bench;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Scenario {
    Idle,
    OneTimer,
    VisibleTimers,
    Repaint,
}

struct Run {
    pages: Pages,
    host: RenderHost,
    media: Media,
    pixels: Vec<u8>,
    time: FrameTime,
    wake: mpsc::Receiver<()>,
    phases: Vec<[u64; 4]>,
    work: Vec<[usize; 2]>,
    repaired: Vec<u64>,
    last_index: usize,
    waits: usize,
}

fn setup(scale: f32, scenario: Scenario, queued: bool, threads: u32) -> Run {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture = true;
    let time = FrameTime {
        instant: Instant::now(),
        wall_time: UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    };
    let now = events::milliseconds(time);
    pages.activity = json!({"fissures": (0..18).map(|i| json!({
        "id":format!("row-{i}"), "tier":"Lith", "mission":"Survival", "node":"Void",
        "hard":i % 2 == 0,
        "expiry":now + if i == 0 || scenario == Scenario::VisibleTimers { 3_500_500 } else { 200_000_500 },
    })).collect::<Vec<_>>()});
    pages.configure(
        ((1000.0 * scale) as u32, (900.0 * scale) as u32),
        ((400.0 * scale) as u32, (900.0 * scale) as u32),
        scale,
    );
    pages.layout_rail_at(time);
    let host = RenderHost::new(HostPolicy {
        threads,
        analysis: retinue_render::AnalysisPolicy::disabled(),
        ..HostPolicy::desktop()
    });
    pages.rail.set_render_host(host.clone());
    let (send, wake) = mpsc::channel();
    if queued {
        pages
            .rail
            .set_layout_execution(retinue_ui::runtime::LayoutExecution::new(
                &host,
                Arc::new(move || {
                    let _ = send.send(());
                }),
            ));
    }
    let mut run = Run {
        pixels: vec![0; (pages.rail.size.0 * pages.rail.size.1 * 4) as usize],
        pages,
        host,
        media: Media::default(),
        time,
        wake,
        phases: Vec::new(),
        work: Vec::new(),
        repaired: Vec::new(),
        last_index: 0,
        waits: 0,
    };
    for _ in 0..8 {
        paint(&mut run, Damage::Full);
    }
    run
}

fn paint(run: &mut Run, damage: Damage) -> [Duration; 2] {
    let repair = damage.regions(run.pages.rail.size);
    if repair.is_empty() {
        return [Duration::ZERO; 2];
    }
    let prepare = Instant::now();
    run.pages.rail.prepare_paint(&run.media);
    run.pages.rail.prepare_rasters(&repair);
    let prepared = prepare.elapsed();
    let native = Instant::now();
    let pass = run.host.begin_frame(bench::render_threads());
    let (width, height) = run.pages.rail.size;
    let mut painter =
        Painter::with_threads(&mut run.pixels, width, height, pass.threads()).unwrap();
    painter.repair(&repair);
    painter.fill_rounded_rect(0, 0, width, height, 0, controls::PANEL);
    run.pages.rail.draw_prepared(&mut painter);
    painter.finish().unwrap();
    [prepared, native.elapsed()]
}

fn step(run: &mut Run, scenario: Scenario, index: usize) {
    // Keep every measured countdown in its seconds-resolution range.
    assert!(
        index < 3_400,
        "rail timer trace supports at most 3,000 samples"
    );
    let elapsed = if matches!(scenario, Scenario::OneTimer | Scenario::VisibleTimers) {
        Duration::from_secs(index as u64)
    } else {
        Duration::ZERO
    };
    let time = run.time.at(run.time.instant + elapsed);
    let start = Instant::now();
    let mut damage = run.pages.rail.update_view(time);
    let mut owner = start.elapsed();
    let mut wait = Duration::ZERO;
    while run.pages.rail.layout_pending() {
        let started = Instant::now();
        run.wake.recv_timeout(Duration::from_secs(10)).unwrap();
        wait += started.elapsed();
        run.waits += 1;
        let started = Instant::now();
        damage = damage.union(run.pages.rail.update_view(time));
        owner += started.elapsed();
    }
    let work = run.pages.rail.scene_update_stats();
    run.work.push([work.layout_computations, work.recordings]);
    if scenario == Scenario::Repaint {
        damage = Damage::Full;
    }
    run.repaired
        .push(damage.regions(run.pages.rail.size).area());
    let [prepare, native] = paint(run, damage);
    run.phases
        .push([owner, wait, prepare, native].map(|t| t.as_nanos() as u64));
    run.last_index = index;
}

fn verify(run: &Run, scenario: Scenario) {
    let mut fresh = setup(run.pages.rail.scale, scenario, false, 1);
    step(&mut fresh, scenario, run.last_index);
    paint(&mut fresh, Damage::Full);
    if let Some(index) = run
        .pixels
        .iter()
        .zip(&fresh.pixels)
        .position(|(a, b)| a != b)
    {
        let pixel = index / 4 * 4;
        let difference: Vec<_> = run
            .pixels
            .iter()
            .zip(&fresh.pixels)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, a.abs_diff(*b)))
            .collect();
        panic!(
            "{scenario:?} scale={} step={} pixel={} actual={:?} expected={:?} different={} max_delta={}",
            run.pages.rail.scale,
            run.last_index,
            pixel / 4,
            &run.pixels[pixel..pixel + 4],
            &fresh.pixels[pixel..pixel + 4],
            difference.len(),
            difference.iter().map(|(_, delta)| *delta).max().unwrap()
        );
    }
}

#[test]
fn rail_update_trace_matches_fresh_layout_and_paint() {
    for scale in [1.0, 1.2] {
        for queued in [false, true] {
            for scenario in [
                Scenario::Idle,
                Scenario::OneTimer,
                Scenario::VisibleTimers,
                Scenario::Repaint,
            ] {
                let mut run = setup(scale, scenario, queued, 2);
                for index in [1, 2, 60, 61] {
                    step(&mut run, scenario, index);
                    verify(&run, scenario);
                }
            }
        }
    }
}

#[test]
#[ignore = "production right-rail updates; make bench BENCH_SUITE=gui-rail-updates"]
fn rail_update_benchmark() {
    let mut cases = Vec::new();
    for scale in [1.0, 1.2, 1.5] {
        for (name, scenario) in [
            ("idle", Scenario::Idle),
            ("one-timer", Scenario::OneTimer),
            ("visible-timers", Scenario::VisibleTimers),
            ("repaint", Scenario::Repaint),
        ] {
            for (suffix, queued) in [("inline", false), ("queued", true)] {
                let name = format!("{name}-{suffix}");
                if !bench::selected(&name) {
                    continue;
                }
                cases.push(bench::measure(
                    &name,
                    ((400.0 * scale) as u32, (900.0 * scale) as u32),
                    scale,
                    || setup(scale, scenario, queued, bench::render_threads().get()),
                    |run, index| step(run, scenario, index),
                    |run| {
                        verify(run, scenario);
                        let phases: serde_json::Map<_, _> =
                            ["owner", "layout_wait", "prepare", "native"]
                                .into_iter()
                                .enumerate()
                                .map(|(column, name)| {
                                    (
                                        name.to_owned(),
                                        json!(
                                            run.phases
                                                .iter()
                                                .skip(6)
                                                .map(|sample| sample[column])
                                                .collect::<Vec<_>>()
                                        ),
                                    )
                                })
                                .collect();
                        json!({"phases_ns":phases, "layout_waits":run.waits,
                            "work_counts": {
                                "layout_computations":run.work.iter().skip(6).map(|v| v[0]).collect::<Vec<_>>(),
                                "recordings":run.work.iter().skip(6).map(|v| v[1]).collect::<Vec<_>>(),
                            },
                            "repair_pixels":run.repaired.iter().skip(6).collect::<Vec<_>>()})
                    },
                ));
            }
        }
    }
    bench::report_with_scope(
        "gui-rail-updates",
        cases,
        "Production compiled Events rail with 18 mounted fissures; owner update, layout completion, preparation and native completion. Fixed clock; no network, remote artwork or platform presentation. Queued cases use the normal shared layout executor.",
    );
}
