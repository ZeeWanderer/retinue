use super::*;
use retinue_render::{
    DrawPacket, HostPolicy, RenderHost,
    damage::Region,
    work::{Graph, Priority},
};
#[cfg(feature = "profiling")]
use retinue_ui::profiling::{Capture, Phase, Sample, Scope};
use std::{
    num::NonZeroUsize,
    sync::{Arc, mpsc},
};

#[allow(dead_code)]
#[path = "../../../../../../tools/benchmarks/support.rs"]
mod bench;

mod listing_phases;
#[cfg(feature = "geometry-census")]
mod solver_trace;

const SIZE: (u32, u32) = (2560, 1440);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Owner,
    Packet,
    Dag,
    Layout,
    Desktop,
}

struct Run {
    shell: Shell,
    pixels: Vec<u8>,
    phases: Vec<[u64; 9]>,
    background_at_submit: Vec<usize>,
    offsets: Vec<u32>,
    wake: mpsc::Receiver<()>,
    layout_waits: usize,
    #[cfg(feature = "geometry-census")]
    geometry_samples: Vec<[usize; 6]>,
    #[cfg(feature = "geometry-census")]
    solver: solver_trace::Trace,
    #[cfg(feature = "profiling")]
    detail: Vec<Sample>,
}

impl Run {
    fn with_solver_trace<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        #[cfg(feature = "geometry-census")]
        if let Some(timed) = self.solver.timed {
            let (out, rows) = retinue_ui::scene::census::solver::capture(timed, || f(self));
            self.solver.last = rows;
            return out;
        }
        f(self)
    }
}

fn setup(page: Page, scale: f32, policy: HostPolicy, mode: Mode) -> Run {
    setup_with(page, scale, policy, mode, |shell| seed_fixture(shell, page))
}

fn setup_with(
    page: Page,
    scale: f32,
    policy: HostPolicy,
    mode: Mode,
    seed: impl FnOnce(&mut Shell),
) -> Run {
    let mut shell = Shell::new(Some(Snapshot::fixture()), scale).unwrap();
    let host = RenderHost::new(HostPolicy {
        analysis: if mode == Mode::Desktop {
            policy.analysis
        } else {
            retinue_render::AnalysisPolicy::disabled()
        },
        ..policy
    });
    shell.render_host = host.clone();
    shell.settings.set_render_host(host.clone());
    shell.foundry.set_render_host(host.clone());
    if page == Page::Foundry
        && let Ok(policy) = std::env::var("GUI_BENCH_MATERIALIZATIONS")
    {
        shell.foundry.benchmark_materializations(&policy);
    }
    shell.pages.ui.set_render_host(host.clone());
    shell.pages.rail.set_render_host(host.clone());
    shell
        .composition
        .renderer
        .borrow_mut()
        .set_host(host.clone());
    shell.composition.set_motion_enabled(false);
    seed(&mut shell);
    shell.resize(SIZE, 1.0).unwrap();
    shell.set_page(page);
    let (send, wake) = mpsc::channel();
    if matches!(mode, Mode::Layout | Mode::Desktop) {
        let execution = retinue_ui::runtime::LayoutExecution::new(
            &host,
            Arc::new(move || {
                let _ = send.send(());
            }),
        );
        shell.pages.ui.set_layout_execution(execution.clone());
        shell.pages.rail.set_layout_execution(execution.clone());
        shell.foundry.set_layout_execution(execution);
    }
    Run {
        shell,
        pixels: vec![0; (SIZE.0 * SIZE.1 * 4) as usize],
        phases: Vec::new(),
        background_at_submit: Vec::new(),
        offsets: Vec::new(),
        wake,
        layout_waits: 0,
        #[cfg(feature = "geometry-census")]
        geometry_samples: Vec::new(),
        #[cfg(feature = "geometry-census")]
        solver: solver_trace::Trace::configured(),
        #[cfg(feature = "profiling")]
        detail: Vec::new(),
    }
}

fn seed_fixture(shell: &mut Shell, page: Page) {
    shell.pages.fixture();
    if page == Page::Relics {
        let source = serde_json::to_value(&shell.pages.relics.items[0]).unwrap();
        let rows: Vec<_> = (0..300)
            .map(|index| {
                let mut row = source.clone();
                row["id"] = json!(format!("relic-{index}"));
                row["name"] = json!(format!("Lith {index:03}"));
                row["rewards"] = json!(
                    (0..6)
                        .map(|n| source["rewards"][n % 2].clone())
                        .collect::<Vec<_>>()
                );
                row
            })
            .collect();
        shell
            .pages
            .relics
            .replace_fixture(json!({"items": rows}), &shell.pages.quotes);
    }
    shell.pages.activity = json!({});
    shell.foundry.update(crate::data::foundry::fixture());
}

fn native(
    mut pixels: Vec<u8>,
    packet: DrawPacket,
    threads: std::num::NonZeroU32,
) -> (Vec<u8>, Instant, Instant) {
    let started = Instant::now();
    let mut painter = Painter::with_threads(&mut pixels, SIZE.0, SIZE.1, threads).unwrap();
    painter.fill_rounded_rect(0, 0, SIZE.0, SIZE.1, 0, BACKGROUND);
    packet.draw(&mut painter);
    painter.finish().unwrap();
    (pixels, started, Instant::now())
}

fn frame(run: &mut Run, mode: Mode, scenario: &str, index: usize) {
    let started = Instant::now();
    #[cfg(feature = "profiling")]
    let capture = Capture::start();
    let shell = &mut run.shell;
    match scenario {
        "relic-scroll" => {
            let delta = if index % 60 < 30 { 24.0 } else { -24.0 };
            shell.pages.wheel(false, delta, false);
            shell.pages.tick(Instant::now());
            run.offsets.push(shell.pages.relics.offset);
        }
        "foundry-reflow" => {
            shell
                .resize((SIZE.0 - (index % 12) as u32 * 8, SIZE.1), 1.0)
                .unwrap();
        }
        "foundry-scroll" => {
            let delta = if index % 60 < 30 { 24.0 } else { -24.0 };
            shell.foundry.wheel(delta, false);
            shell.foundry.tick(Instant::now());
            run.offsets.push(shell.foundry.offset);
        }
        _ => (),
    }
    let mut update = started.elapsed();
    let mut layout_wait = Duration::ZERO;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let poll = Instant::now();
        let pages = shell.pages.poll_layout();
        let foundry = shell.foundry.poll_layout();
        update += poll.elapsed();
        if pages && foundry {
            break;
        }
        run.layout_waits += 1;
        let wait = Instant::now();
        {
            #[cfg(feature = "profiling")]
            let _scope = Scope::enter(Phase::LayoutWait);
            run.wake
                .recv_timeout(deadline.saturating_duration_since(wait))
                .unwrap();
        }
        layout_wait += wait.elapsed();
    }
    let recording = Instant::now();
    shell.prepare_paint();
    let record = recording.elapsed();
    let host = shell.render_host();
    let _pass = host.begin_pass(0);
    let threads = host.task_threads(bench::render_threads());
    let repair = Region::from(Rect {
        width: SIZE.0,
        height: SIZE.1,
        ..Rect::default()
    });
    let prepare = Instant::now();
    let mut owner = update + record;
    let mut packet_wait = Duration::ZERO;
    let mut native_dispatch = Duration::ZERO;
    let mut native_return = Duration::ZERO;
    let native_time;
    if mode == Mode::Owner {
        {
            #[cfg(feature = "profiling")]
            let _scope = Scope::enter(Phase::Prepare);
            shell.prepare_rasters(&repair);
        }
        owner += prepare.elapsed();
        #[cfg(feature = "profiling")]
        let _scope = Scope::enter(Phase::Native);
        let frame = host.begin_frame(threads);
        let paint = Instant::now();
        let mut painter =
            Painter::with_threads(&mut run.pixels, SIZE.0, SIZE.1, frame.threads()).unwrap();
        painter.fill_rounded_rect(0, 0, SIZE.0, SIZE.1, 0, BACKGROUND);
        shell.draw_prepared(&mut painter);
        painter.finish().unwrap();
        native_time = paint.elapsed();
        owner += native_time;
    } else {
        let (send, receive) = std::sync::mpsc::channel();
        let mut pending = {
            #[cfg(feature = "profiling")]
            let _scope = Scope::enter(Phase::Prepare);
            shell
                .prepare_packet(
                    &repair,
                    Arc::new(move || {
                        let _ = send.send(());
                    }),
                )
                .unwrap()
        };
        owner += prepare.elapsed();
        let deadline = Instant::now() + Duration::from_secs(10);
        let packet = loop {
            let publish = Instant::now();
            let result = {
                #[cfg(feature = "profiling")]
                let _scope = Scope::enter(Phase::Prepare);
                pending.poll()
            };
            owner += publish.elapsed();
            if let Some(result) = result {
                break result.unwrap();
            }
            {
                let wait = Instant::now();
                #[cfg(feature = "profiling")]
                let _scope = Scope::enter(Phase::PrepareWait);
                receive
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap();
                packet_wait += wait.elapsed();
            }
        };
        let pixels = std::mem::take(&mut run.pixels);
        if mode == Mode::Packet {
            #[cfg(feature = "profiling")]
            let _scope = Scope::enter(Phase::Native);
            let frame = host.begin_frame(threads);
            let (pixels, begin, end) = native(pixels, packet, frame.threads());
            let elapsed = end - begin;
            run.pixels = pixels;
            native_time = elapsed;
            owner += elapsed;
        } else {
            let executor = host.executor();
            run.background_at_submit
                .push(executor.stats().background_participants);
            let submit = Instant::now();
            #[cfg(feature = "profiling")]
            let submission_scope = Scope::enter(Phase::Submit);
            let mut graph = Graph::default();
            let mut output = graph.add_owned(
                &[],
                NonZeroUsize::new(threads.get() as usize).unwrap(),
                move |_| Ok(native(pixels, packet, threads)),
            );
            let job = executor.submit(graph, Priority::Foreground, || {}).unwrap();
            owner += submit.elapsed();
            #[cfg(feature = "profiling")]
            drop(submission_scope);
            // Only this headless benchmark waits; production wakes the platform owner.
            {
                #[cfg(feature = "profiling")]
                let _scope = Scope::enter(Phase::NativeWait);
                job.wait().unwrap();
            }
            let collect = Instant::now();
            #[cfg(feature = "profiling")]
            let _scope = Scope::enter(Phase::Submit);
            let (pixels, begin, end) = output.take().unwrap().unwrap();
            run.pixels = pixels;
            native_time = end - begin;
            native_dispatch = begin - submit;
            native_return = collect - end;
            owner += collect.elapsed();
        }
    }
    shell.prepared();
    #[cfg(feature = "profiling")]
    let detail = capture.finish();
    let total = started.elapsed();
    #[cfg(feature = "profiling")]
    {
        assert_eq!(detail.phases_ns.iter().sum::<u64>(), detail.total_ns);
        run.detail.push(detail);
    }
    run.phases.push(
        [
            update,
            layout_wait,
            record,
            owner,
            native_time,
            total,
            packet_wait,
            native_dispatch,
            native_return,
        ]
        .map(|n| n.as_nanos() as u64),
    );
}

fn verify(run: &Run) {
    if !run.offsets.is_empty() {
        assert!(
            run.offsets.windows(2).filter(|w| w[0] != w[1]).count() > run.offsets.len() / 2,
            "scroll benchmark must exercise moving content"
        );
    }
    let mut expected = vec![0; run.pixels.len()];
    let mut painter =
        Painter::with_threads(&mut expected, SIZE.0, SIZE.1, bench::render_threads()).unwrap();
    painter.fill_rounded_rect(0, 0, SIZE.0, SIZE.1, 0, BACKGROUND);
    run.shell.draw_prepared(&mut painter);
    painter.finish().unwrap();
    assert_eq!(
        run.pixels.iter().zip(&expected).position(|(a, b)| a != b),
        None
    );
}

#[test]
#[ignore = "full shell preparation and native execution; make bench BENCH_SUITE=gui-execution"]
fn shell_execution_benchmark() {
    let mut cases = Vec::new();
    let mut modes = [
        ("owner", Mode::Owner),
        ("packet", Mode::Packet),
        ("dag", Mode::Dag),
        ("layout", Mode::Layout),
        ("desktop", Mode::Desktop),
    ];
    let run: usize = std::env::var("RETINUE_BENCH_RUN_INDEX")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    let mode_count = modes.len();
    modes.rotate_left(run % mode_count);
    for scale in crate::app::benchmark::scales(&[1.0, 1.2]) {
        for (scenario, page) in [
            ("foundry-repaint", Page::Foundry),
            ("foundry-reflow", Page::Foundry),
            ("foundry-scroll", Page::Foundry),
            ("relic-scroll", Page::Relics),
        ] {
            for (label, mode) in modes {
                let name = format!("{scenario}-{label}");
                if !bench::selected(&name) {
                    continue;
                }
                cases.push(bench::measure_with_warmup(
                    &name,
                    SIZE,
                    scale as f32,
                    10,
                    || setup(page, scale as f32, HostPolicy {
                        threads: bench::render_threads().get(),
                        ..HostPolicy::desktop()
                    }, mode),
                    |run, index| {
                        #[cfg(not(feature = "geometry-census"))]
                        frame(run, mode, scenario, index);
                        #[cfg(feature = "geometry-census")]
                        {
                            let (_, c) = retinue_ui::scene::census::measure(|| run.with_solver_trace(|r| frame(r, mode, scenario, index)));
                            run.solver.record(scenario);
                            run.geometry_samples.push([c.world_resolves, c.restored_slots, c.paint_placements,
                                c.bounds_calls, c.bounds_keys.len(), c.slice_requests]);
                        }
                    },
                    |run| {
                        verify(run);
                        if matches!(mode, Mode::Layout | Mode::Desktop) && scenario == "foundry-reflow" {
                            assert!(run.layout_waits > 0, "must exercise queued layout");
                        }
                        let mut phases = serde_json::Map::new();
                        for (column, name) in
                            ["update", "layout_wait", "record", "owner", "native", "total", "packet_wait", "native_dispatch", "native_return"].iter().enumerate()
                        {
                            let values: Vec<_> =
                                run.phases.iter().skip(11).map(|n| n[column]).collect();
                            phases.insert((*name).into(), json!(values));
                        }
                        let resources = json!({"phases_ns": phases,
                            "layout_waits": run.layout_waits,
                            "analysis_submitted": run.shell.render_host().stats().analysis.submitted,
                            "background_at_native_submit": run.background_at_submit.iter().skip(11).collect::<Vec<_>>(),
                            "render_threads": run.shell.render_host().task_threads(bench::render_threads()).get()});
                        #[cfg(feature = "geometry-census")]
                        let resources = {
                            let mut resources = resources;
                            run.solver.attach(&mut resources, 11);
                            resources["geometry_census"] = json!({
                                "columns": ["world_resolves", "restored_slots", "paint_placements",
                                    "bounds_calls", "distinct_bounds_keys", "slice_requests"],
                                "rows": &run.geometry_samples[11..],
                            });
                            resources
                        };
                        #[cfg(feature = "profiling")]
                        let resources = {
                            let mut resources = resources;
                            let mut exclusive = serde_json::Map::new();
                            for (column, name) in retinue_ui::profiling::NAMES.iter().enumerate() {
                                let values: Vec<_> = run.detail.iter().skip(11)
                                    .map(|sample| sample.phases_ns[column]).collect();
                                exclusive.insert((*name).into(), json!(values));
                            }
                            exclusive.insert("total".into(), json!(run.detail.iter().skip(11)
                                .map(|sample| sample.total_ns).collect::<Vec<_>>()));
                            resources["owner_exclusive_ns"] = json!(exclusive);
                            #[cfg(feature = "layout-profiling")]
                            {
                                let mut phases = serde_json::Map::new();
                                for (i, name) in retinue_ui::profiling::layout::PHASES.iter().enumerate() {
                                    phases.insert((*name).into(), json!(run.detail.iter().skip(11)
                                        .map(|s| s.layout.ns[i]).collect::<Vec<_>>()));
                                }
                                resources["layout_job_ns"] = json!(phases);
                                resources["work_counts"] = json!({
                                    "layout_jobs": run.detail.iter().skip(11).map(|s| s.layout.jobs).collect::<Vec<_>>(),
                                    "layout_groups": run.detail.iter().skip(11).map(|s| s.layout.groups).collect::<Vec<_>>(),
                                    "layout_slots": run.detail.iter().skip(11).map(|s| s.layout.slots).collect::<Vec<_>>(),
                                    "layout_computations": run.detail.iter().skip(11).map(|s| s.layout.computed).collect::<Vec<_>>(),
                                });
                            }
                            #[cfg(feature = "ir-profiling")]
                            {
                                let mut phases = serde_json::Map::new();
                                let mut counts = serde_json::Map::new();
                                for (i, name) in retinue_ui::profiling::IR_NAMES.iter().enumerate() {
                                    phases.insert((*name).into(), json!(run.detail.iter().skip(11)
                                        .map(|s| s.ir.ns[i]).collect::<Vec<_>>()));
                                    counts.insert(format!("ir_{name}"), json!(run.detail.iter().skip(11)
                                        .map(|s| s.ir.calls[i]).collect::<Vec<_>>()));
                                }
                                resources["owner_ir_ns"] = json!(phases);
                                if let Some(existing) = resources["work_counts"].as_object_mut() {
                                    existing.extend(counts);
                                } else {
                                    resources["work_counts"] = json!(counts);
                                }
                            }
                            resources
                        };
                        resources
                    },
                ));
            }
        }
    }
    bench::report_with_scope(
        "gui-execution",
        cases,
        "Full fixture frames, no compositor or remote artwork. Owner uses synchronous raster; packet prepares asynchronously; DAG also queues native; layout also queues layout; desktop enables normal background analysis on the same path. Owner excludes completion waits. Native dispatch includes graph construction/admission to worker start; native return ends when the benchmark thread resumes, not when a desktop event is delivered.",
    );
}

#[test]
fn queued_shell_layout_matches_inline_frames() {
    for (scenario, page) in [
        ("foundry-reflow", Page::Foundry),
        ("foundry-scroll", Page::Foundry),
        ("relic-scroll", Page::Relics),
    ] {
        // Compare layout output without time-budgeted cache admission changing
        // direct versus cached text rounding between independent hosts.
        let policy = HostPolicy {
            threads: 2,
            preparation_tasks: 0,
            ..HostPolicy::desktop()
        };
        let mut inline = setup(page, 1.2, policy, Mode::Dag);
        let mut queued = setup(page, 1.2, policy, Mode::Layout);
        for index in 0..4 {
            frame(&mut inline, Mode::Dag, scenario, index);
            frame(&mut queued, Mode::Layout, scenario, index);
            let differences: Vec<_> = inline
                .pixels
                .chunks_exact(4)
                .zip(queued.pixels.chunks_exact(4))
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| ((i as u32 % SIZE.0, i as u32 / SIZE.0), a, b))
                .collect();
            assert!(
                differences.is_empty(),
                "{scenario}, frame {index}: {} differing pixels, first {:?}",
                differences.len(),
                &differences[..differences.len().min(8)]
            );
        }
    }
}
