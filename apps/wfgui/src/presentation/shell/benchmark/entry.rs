use super::*;
use retinue_render::surface::Aged;
use retinue_ui::desktop::winit::{dpi::PhysicalPosition, event::MouseScrollDelta};
use std::{num::NonZeroU32, sync::Arc};

const SIZE: (u32, u32) = (1680, 1050);

struct Trial {
    shell: Shell,
    surface: Aged,
    buffers: [Vec<u8>; 3],
    frames: usize,
    layout_ready: std::sync::mpsc::Receiver<()>,
}

fn click(shell: &mut Shell, control: Control) -> (Damage, Duration) {
    let (_, r, _) = shell
        .controls()
        .into_iter()
        .find(|c| c.0 == control)
        .unwrap();
    let pos = (
        f64::from(r.x) + f64::from(r.width) / 2.,
        f64::from(r.y) + f64::from(r.height) / 2.,
    );
    let motion = shell.pointer_move(pos);
    let down = shell.pointer_button(true, Instant::now());
    let start = Instant::now();
    let up = shell.pointer_button(false, start);
    let release = start.elapsed();
    (motion.union(down).union(up), release)
}

fn scroll(shell: &mut Shell, pixels: f64) -> Damage {
    shell.center_wheel(
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0., -pixels)),
        Instant::now(),
    )
}

impl Trial {
    fn new(page: Page, scale: f32, eager: bool) -> Self {
        let mut shell = Shell::new(Some(Snapshot::fixture()), scale).unwrap();
        let (tx, layout_ready) = std::sync::mpsc::channel();
        let execution = retinue_ui::runtime::LayoutExecution::new(
            &shell.render_host(),
            Arc::new(move || {
                let _ = tx.send(());
            }),
        );
        shell.pages.ui.set_layout_execution(execution.clone());
        shell.pages.rail.set_layout_execution(execution.clone());
        shell.foundry.set_layout_execution(execution.clone());
        shell.pages.relics_view.eager_grid = eager;
        shell.pages.mastery_view.eager_grid = eager;
        shell.pages.fixture();
        shell.pages.activity = json!({});
        let listing = match page {
            Page::Relics => &mut shell.pages.relics,
            Page::Mastery => &mut shell.pages.mastery,
            _ => unreachable!(),
        };
        let base = &listing.items[0];
        let rows: Vec<_> = (0..120)
            .map(|n| {
                let mut row = base.clone();
                row.id = format!("item-{n}");
                row.name = format!("Item {n:03}");
                row
            })
            .collect();
        listing.replace_fixture(json!({"items":rows}), &shell.pages.quotes);
        shell.resize(SIZE, 1.).unwrap();
        Self {
            shell,
            surface: Aged::default(),
            frames: 0,
            layout_ready,
            buffers: std::array::from_fn(|_| vec![0; (SIZE.0 * SIZE.1 * 4) as usize]),
        }
    }

    fn frame(
        &mut self,
        force: bool,
        input: impl FnOnce(&mut Shell) -> Damage,
    ) -> serde_json::Value {
        let threads = NonZeroU32::new(4).unwrap();
        #[cfg(feature = "geometry-census")]
        crate::app::benchmark::entry_cost::take();
        let start = Instant::now();
        let damage = input(&mut self.shell);
        let input_end = Instant::now();
        #[cfg(feature = "geometry-census")]
        let input_child = crate::app::benchmark::entry_cost::elapsed(
            crate::app::benchmark::entry_cost::Stage::Child,
        );
        let mut waits = 0u32;
        let mut waiting = Duration::ZERO;
        while !(self.shell.pages.poll_layout() & self.shell.foundry.poll_layout()) {
            let wait = Instant::now();
            self.layout_ready
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            waiting += wait.elapsed();
            waits += 1;
        }
        let layout_end = Instant::now();
        self.shell.prepare_paint();
        let prepared = Instant::now();
        crate::app::benchmark::measure_demand(&self.shell);
        let demand_end = Instant::now();
        let snapshot = self.shell.paint_scene();
        let scene_end = Instant::now();
        let event_pixels = damage.regions(SIZE).area();
        self.surface
            .invalidate(if force { Damage::Full } else { damage });
        let slot = self.frames % 3;
        let [target, latest] = self
            .buffers
            .get_disjoint_mut([slot, (slot + 2) % 3])
            .unwrap();
        let frame = self
            .surface
            .plan_queued(
                if self.frames < 3 { 0 } else { 3 },
                SIZE,
                Some(snapshot),
                target,
                (self.frames != 0).then_some(&latest[..]),
                threads,
            )
            .unwrap()
            .unwrap();
        let repair = frame.repair().area();
        let plan_end = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut job = self
            .shell
            .prepare_packet(
                frame.repair(),
                Arc::new(move || {
                    let _ = tx.send(());
                }),
            )
            .unwrap();
        let packet = loop {
            if let Some(result) = job.poll() {
                break result.unwrap();
            }
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
        };
        let packet_end = Instant::now();
        self.shell.prepared();
        let submitted = frame
            .execute(threads, |p, _| {
                p.fill_rounded_rect(0, 0, SIZE.0, SIZE.1, 0, BACKGROUND);
                packet.draw(p);
                Ok(())
            })
            .unwrap();
        let end = Instant::now();
        let present = submitted.damage.area();
        assert!(self.surface.submitted(submitted));
        self.frames += 1;
        let report = json!({"input_ns":(input_end-start).as_nanos(), "prepare_ns":(prepared-input_end).as_nanos(),
            "layout_owner_ns":(layout_end-input_end-waiting).as_nanos(), "wake_wait_ns":waiting.as_nanos(),
            "wake_waits":waits, "prepare_paint_ns":(prepared-layout_end).as_nanos(),
            "demand_ns":(demand_end-prepared).as_nanos(),
            "scene_ns":(scene_end-demand_end).as_nanos(), "plan_ns":(plan_end-scene_end).as_nanos(),
            "packet_ns":(packet_end-plan_end).as_nanos(), "native_ns":(end-packet_end).as_nanos(),
            "total_ns":(end-start).as_nanos(),"event_px":event_pixels,"present_px":present,"repair_px":repair});
        #[cfg(feature = "geometry-census")]
        let report = {
            let mut report = report;
            #[cfg(feature = "layout-profiling")]
            if let Some((cs, ce)) = crate::app::benchmark::entry_cost::CHILD_SOLVE.take() {
                let parent = self.shell.pages.ui.view_profile().2.last_worker_window;
                let overlap = parent
                    .filter(|(rs, _)| *rs >= start)
                    .map_or(Duration::ZERO, |(rs, re)| {
                        ce.min(re).saturating_duration_since(cs.max(rs))
                    });
                report["child_worker_ns"] = json!((ce - cs).as_nanos());
                report["root_child_worker_overlap_ns"] = json!(overlap.as_nanos());
            }
            #[cfg(feature = "profiling")]
            if let Some(times) = crate::app::benchmark::entry_cost::take_first_child() {
                for (name, time) in [
                    "evaluation",
                    "lowering",
                    "solve",
                    "geometry",
                    "frame",
                    "projection",
                    "input",
                ]
                .into_iter()
                .zip(times)
                {
                    report[format!("first_child_{name}_ns")] = json!(time.as_nanos());
                }
            }
            report["input_child_ns"] = json!(input_child.as_nanos());
            for (key, time) in ["root_ns", "child_ns", "targets_ns", "bind_ns", "rail_ns"]
                .into_iter()
                .zip(crate::app::benchmark::entry_cost::take())
            {
                report[key] = json!(time.as_nanos());
            }
            report
        };
        report
    }

    fn visit(&mut self, page: Page, force: bool, continued: bool) -> serde_json::Value {
        self.frame(false, |shell| click(shell, Control::Inventory).0);
        assert_eq!(self.shell.page, Page::Inventory);
        let mut release = Duration::ZERO;
        let mut entry = self.frame(force, |shell| {
            let (damage, elapsed) = click(
                shell,
                match page {
                    Page::Mastery => Control::Mastery,
                    Page::Relics => Control::Relics,
                    _ => unreachable!(),
                },
            );
            release = elapsed;
            damage
        });
        entry["release_ns"] = json!(release.as_nanos());
        assert_eq!(self.shell.page, page);
        assert_eq!(self.shell.pages.listing().offset, 0);
        let r = self.shell.capture_bounds("page.grid").unwrap();
        let hover = self.frame(false, |shell| {
            shell.pointer_move(((r.x + 20) as f64, (r.y + 20) as f64))
        });
        let wheel = self.frame(force, |shell| scroll(shell, 8.));
        assert_eq!(self.shell.pages.listing().offset, 8);
        let mut result = json!({"entry":entry, "hover":hover, "wheel":wheel});
        if continued {
            let mut frames = Vec::new();
            for _ in 0..6 {
                frames.push(self.frame(false, |shell| scroll(shell, 24.)));
            }
            assert_eq!(self.shell.pages.listing().offset, 152);
            result["scroll"] = json!(frames);
        }
        let offset = self.shell.pages.listing().offset;
        scroll(&mut self.shell, -f64::from(offset));
        result
    }
}

#[test]
#[ignore = "release page navigation and first-scroll attribution"]
fn listing_entry_and_first_scroll() {
    let mut samples = Vec::new();
    let reverse = usize::from(std::env::var_os("GUI_BENCH_REVERSE").is_some());
    for scale in crate::app::benchmark::scales(&[1., 1.2]) {
        for page in [Page::Relics, Page::Mastery] {
            let mut trials = [true, false].map(|eager| Trial::new(page, scale as f32, eager));
            for cycle in 0..24 {
                let force = (cycle / 2 + reverse) % 2 != 0;
                for index in 0..2 {
                    let variant = (index + cycle + reverse) % 2;
                    let visit = trials[variant].visit(page, force, false);
                    if cycle >= 4 {
                        samples.push(
                            json!({"page":format!("{page:?}"),"scale":scale,"force":force,
                            "retained":variant==1,"cycle":cycle,"phases":visit}),
                        );
                    }
                }
            }
        }
    }
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&samples).unwrap(),
    )
    .unwrap();
}

#[test]
fn queued_mount_preserves_visible_grid_and_hits() {
    for page in [Page::Relics, Page::Mastery] {
        for scale in [1., 1.2] {
            let mut trials = [false, true].map(|queue| {
                let mut t = Trial::new(page, scale, false);
                if !queue {
                    t.shell.pages.ui.use_inline_mounts();
                }
                t
            });
            let control = if page == Page::Relics {
                Control::Relics
            } else {
                Control::Mastery
            };
            for step in 0..6 {
                for t in &mut trials {
                    t.frame(false, |shell| match step {
                        0 => click(shell, Control::Inventory).0,
                        1 => click(shell, control).0,
                        2 => scroll(shell, 148.),
                        3 => {
                            click(shell, Control::Inventory);
                            let damage = click(shell, control).0;
                            // A newer offset arrives before the fresh layout is collected.
                            damage.union(scroll(shell, -92.))
                        }
                        4 => {
                            click(shell, Control::Inventory);
                            click(shell, control);
                            shell.pages.poll_layout();
                            shell.resize(SIZE, 1.05).unwrap();
                            Damage::Full
                        }
                        _ => {
                            click(shell, Control::Inventory);
                            click(shell, control);
                            shell.pages.poll_layout();
                            // Abandon a preparing generation and revisit the same slot.
                            click(shell, Control::Inventory);
                            click(shell, control).0
                        }
                    });
                }
                if step == 0 {
                    continue;
                }
                let [a, b] = &mut trials;
                let grid = a.shell.capture_bounds("page.grid").unwrap();
                assert_eq!(grid, b.shell.capture_bounds("page.grid").unwrap());
                let slot = (a.frames - 1) % 3;
                for y in grid.y..grid.y + grid.height {
                    let first = ((y * SIZE.0 + grid.x) * 4) as usize;
                    let range = first..first + grid.width as usize * 4;
                    assert!(
                        a.buffers[slot][range.clone()] == b.buffers[slot][range],
                        "{page:?} {scale} step {step} y={y}"
                    );
                }
                let mut hit = false;
                for y in (grid.y..grid.y + grid.height).step_by(107) {
                    for x in (grid.x..grid.x + grid.width).step_by(131) {
                        for t in [&mut *a, &mut *b] {
                            t.shell.pointer_move((x.into(), y.into()));
                        }
                        let action = &a.shell.pages.ui.hover;
                        assert_eq!(action, &b.shell.pages.ui.hover);
                        hit |= action.is_some();
                    }
                }
                assert!(hit);
            }
        }
    }
}

#[test]
#[ignore = "compare eager, serial mount and production queued mount on normal damage"]
fn listing_mount_topology() {
    let mut records = Vec::new();
    let reverse = std::env::var_os("GUI_BENCH_REVERSE").is_some();
    for scale in crate::app::benchmark::scales(&[1., 1.2]) {
        for page in [Page::Relics, Page::Mastery] {
            let mut trials =
                [true, false, false].map(|eager| Trial::new(page, scale as f32, eager));
            trials[1].shell.pages.ui.use_inline_mounts();
            let n = trials.len();
            for cycle in 0..12 + n {
                for index in 0..n {
                    let i = (cycle + if reverse { n - 1 - index } else { index }) % n;
                    let phases = trials[i].visit(page, false, true);
                    if cycle >= n {
                        records.push(
                            json!({"page": format!("{page:?}"), "scale":scale, "cycle":cycle,
                            "strategy":(["eager","serial","queued"][i]),"phases":phases}),
                        );
                    }
                }
            }
        }
    }
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&records).unwrap(),
    )
    .unwrap();
}
