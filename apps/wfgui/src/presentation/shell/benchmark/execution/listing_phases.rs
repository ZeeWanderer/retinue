use super::*;
use retinue_render::surface::Aged;
use retinue_ui::desktop::winit::{dpi::PhysicalPosition, event::MouseScrollDelta};

mod catalog_drag;

struct Buffers {
    aged: Aged,
    pixels: [Vec<u8>; 3],
    count: usize,
    #[cfg(all(feature = "geometry-census", feature = "profiling"))]
    last_update: Option<(
        crate::app::benchmark::entry_cost::recurring::Sample,
        [u64; 5],
    )>,
}

impl Buffers {
    fn new() -> Self {
        Self {
            aged: Aged::default(),
            count: 0,
            #[cfg(all(feature = "geometry-census", feature = "profiling"))]
            last_update: None,
            pixels: std::array::from_fn(|_| vec![0; (SIZE.0 * SIZE.1 * 4) as usize]),
        }
    }
    fn frame(&mut self, run: &mut Run, input: impl FnOnce(&mut Shell) -> Damage) -> [u64; 6] {
        run.with_solver_trace(|run| self.paint_frame(run, input))
    }

    fn paint_frame(&mut self, run: &mut Run, input: impl FnOnce(&mut Shell) -> Damage) -> [u64; 6] {
        let start = Instant::now();
        let shell = &mut run.shell;
        let damage = input(shell);
        let deadline = start + Duration::from_secs(10);
        while !(shell.pages.poll_layout() & shell.foundry.poll_layout()) {
            run.wake
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
        let layout = Instant::now();
        #[cfg(all(feature = "geometry-census", feature = "profiling"))]
        {
            use crate::app::benchmark::entry_cost::recurring;
            self.last_update = recurring::sampling().then(recurring::finish);
        }
        shell.prepare_paint();
        let scene = shell.paint_scene();
        self.aged.invalidate(damage);
        let slot = self.count % 3;
        let [dst, src] = self
            .pixels
            .get_disjoint_mut([slot, (slot + 2) % 3])
            .unwrap();
        let threads = bench::render_threads();
        let Some(frame) = self
            .aged
            .plan_queued(
                if self.count < 3 { 0 } else { 3 },
                SIZE,
                Some(scene),
                dst,
                (self.count != 0).then_some(&src[..]),
                threads,
            )
            .unwrap()
        else {
            let end = Instant::now();
            return [
                (end - start).as_nanos() as u64,
                (layout - start).as_nanos() as u64,
                (end - layout).as_nanos() as u64,
                0,
                0,
                0,
            ];
        };
        let repair_px = frame.repair().area();
        let (tx, rx) = mpsc::channel();
        let mut packet = shell
            .prepare_packet(
                frame.repair(),
                Arc::new(move || {
                    let _ = tx.send(());
                }),
            )
            .unwrap();
        let packet = loop {
            if let Some(result) = packet.poll() {
                break result.unwrap();
            }
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        };
        shell.prepared();
        let native = Instant::now();
        let submission = frame
            .execute(threads, |p, _| {
                p.fill_rounded_rect(0, 0, SIZE.0, SIZE.1, 0, BACKGROUND);
                packet.draw(p);
                Ok(())
            })
            .unwrap();
        let end = Instant::now();
        let present_px = submission.damage.area();
        assert!(self.aged.submitted(submission));
        self.count += 1;
        [
            (end - start).as_nanos() as u64,
            (layout - start).as_nanos() as u64,
            (native - layout).as_nanos() as u64,
            (end - native).as_nanos() as u64,
            repair_px,
            present_px,
        ]
    }
}

fn visible_rows(shell: &Shell) -> (u32, u32) {
    let l = shell.pages.listing();
    let stride = l.grid.height + l.grid.gap;
    (
        l.offset / stride,
        (l.offset + l.viewport.height - 1) / stride,
    )
}

#[test]
#[ignore = "release listing interaction tails across renderer revisions"]
fn listing_phase_regression() {
    let mut reports = Vec::new();
    for scale in crate::app::benchmark::scales(&[1.0, 1.2]) {
        for page in [Page::Relics, Page::Mastery] {
            let mut run = setup(
                page,
                scale as f32,
                HostPolicy {
                    threads: bench::render_threads().get(),
                    ..HostPolicy::desktop()
                },
                Mode::Desktop,
            );
            if page == Page::Mastery {
                let listing = &mut run.shell.pages.mastery;
                let rows: Vec<_> = (0..300)
                    .map(|n| {
                        let mut item = listing.items[0].clone();
                        item.id = format!("item-{n}");
                        item.name = format!("Item {n:03}");
                        item
                    })
                    .collect();
                listing.replace_fixture(json!({"items": rows}), &run.shell.pages.quotes);
                run.shell.pages.dirty = true;
            }
            let eager = std::env::var_os("GUI_BENCH_EAGER_GRID").is_some();
            if eager {
                run.shell.set_page(Page::Inventory);
                run.shell.pages.relics_view.eager_grid = true;
                run.shell.pages.mastery_view.eager_grid = true;
                run.shell.set_page(page);
            }
            let mut buffers = Buffers::new();
            for _ in 0..3 {
                buffers.frame(&mut run, |_| Damage::Full);
            }
            let mut samples = Vec::new();
            #[cfg(all(feature = "geometry-census", feature = "profiling"))]
            let mut scroll_work = Vec::new();
            #[cfg(feature = "layout-profiling")]
            let mut text = crate::app::benchmark::ParagraphFrames::enabled();
            for i in 0..224 {
                #[cfg(all(feature = "geometry-census", feature = "profiling"))]
                let member_window = (page == Page::Relics).then(|| run.shell.pages.relic_window());
                let before = visible_rows(&run.shell);
                let offset = run.shell.pages.listing().offset;
                let delta = if i % 32 < 16 { 24.0 } else { -24.0 };
                #[cfg(all(feature = "geometry-census", feature = "profiling"))]
                crate::app::benchmark::entry_cost::recurring::start();
                let times = buffers.frame(&mut run, |shell| {
                    let (dy, smooth) = shell.wheel_delta(MouseScrollDelta::PixelDelta(
                        PhysicalPosition::new(0., -delta),
                    ));
                    shell.pages.wheel(false, dy, smooth);
                    let damage = shell.pages.tick(Instant::now()).0;
                    shell.page_damage(damage)
                });
                let new_offset = run.shell.pages.listing().offset;
                #[cfg(feature = "layout-profiling")]
                if let Some(text) = &mut text {
                    text.finish_frame();
                }
                assert_eq!(
                    new_offset as i64 - offset as i64,
                    delta as i64,
                    "{page:?} {scale} scroll {i}"
                );
                if i >= 32 {
                    let phase = if before == visible_rows(&run.shell) {
                        "steady"
                    } else {
                        "crossing"
                    };
                    samples.push((phase, times));
                    #[cfg(feature = "geometry-census")]
                    run.solver.record(phase);
                }
                #[cfg(all(feature = "geometry-census", feature = "profiling"))]
                {
                    let work = buffers.last_update.take().unwrap();
                    if i >= 32 {
                        scroll_work.push((
                            member_window.map(|before| (before, run.shell.pages.relic_window())),
                            work,
                        ));
                    }
                }
            }
            let center = run.shell.geometry.center;
            let candidates: Vec<_> = run
                .shell
                .pages
                .ui
                .buttons
                .iter()
                .filter(|b| {
                    b.key.starts_with("list:item:")
                        && b.key.ends_with(":menu")
                        && b.rect.height > 24
                })
                .map(|b| {
                    (
                        (center.x + b.rect.x + 6) as f64,
                        (center.y + b.rect.y + 12) as f64,
                    )
                })
                .collect();
            let mut points = Vec::new();
            let mut targets = Vec::new();
            for point in candidates {
                run.shell.pointer_move(point);
                if let Some(hit) = &run.shell.pages.ui.hover
                    && hit.starts_with("list:item:")
                    && !targets.contains(hit)
                {
                    points.push(point);
                    targets.push(hit.clone());
                    if points.len() == 2 {
                        break;
                    }
                }
            }
            assert_eq!(points.len(), 2, "{page:?} {scale}: need distinct card hits");
            for i in 0..112 {
                let k = i % 2;
                let times = buffers.frame(&mut run, |shell| {
                    shell
                        .pointer_move(points[k])
                        .union(shell.tick(Instant::now()))
                });
                let target = run
                    .shell
                    .pages
                    .ui
                    .hover
                    .as_deref()
                    .expect("hover should hit a card");
                assert!(target.starts_with("list:item:"));
                assert_eq!(targets[k], target, "{page:?} {scale} hover {i}");
                if i >= 16 {
                    samples.push(("hover", times));
                    #[cfg(feature = "geometry-census")]
                    run.solver.record("hover");
                }
            }
            assert_ne!(targets[0], targets[1]);
            for i in 0..112 {
                let times = buffers.frame(&mut run, |_| Damage::Full);
                if i >= 16 {
                    samples.push(("repaint", times));
                    #[cfg(feature = "geometry-census")]
                    run.solver.record("repaint");
                }
            }
            let grid = &run.shell.pages.listing().grid;
            reports.push(json!({"page":format!("{page:?}"),"scale":scale,"size":SIZE,
                "grid_host":if eager {"eager-control"} else {"production"},
                "grid":[grid.columns,grid.height,grid.gap],"targets":targets,
                "columns":["total_ns","input_layout_ns","prepare_packet_ns","native_ns","repair_px","present_px"],
                "samples":samples}));
            #[cfg(feature = "geometry-census")]
            run.solver.attach(reports.last_mut().unwrap(), 0);
            #[cfg(feature = "layout-profiling")]
            if let Some(text) = text {
                text.attach(reports.last_mut().unwrap());
            }
            #[cfg(all(feature = "geometry-census", feature = "profiling"))]
            {
                reports.last_mut().unwrap()["scroll_work"] = json!(scroll_work);
            }
        }
    }
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&reports).unwrap(),
    )
    .unwrap();
}
