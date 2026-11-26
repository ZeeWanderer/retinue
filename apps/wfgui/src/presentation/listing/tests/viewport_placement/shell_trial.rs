use super::*;
use crate::{
    data::storage::Snapshot,
    presentation::shell::{BACKGROUND, Shell},
};
use retinue_render::PacketJob;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

const SIZE: (u32, u32) = (1680, 1050);
const CHILD: Key = Key::new("test.relic.viewport", 0);

#[cfg(feature = "geometry-census")]
mod census;

fn sample_offset(scenario: &str, frame: usize) -> u32 {
    match scenario.split('-').next() {
        Some("offset") => [8, 12][frame % 2],
        Some("crossings") => 8 + (frame % 24).abs_diff(12) as u32 * 24,
        _ => 8,
    }
}

#[derive(Clone, PartialEq)]
struct GridSlot(State<Metrics>);

#[view]
impl View for GridSlot {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        place(
            Canvas(retinue_ui::drawing::ExternalDrawing(CHILD)),
            self.0.get().viewport(),
        )
        .named(VIEWPORT)
    }
}

struct Trial {
    shell: Shell,
    grid: Option<Rc<RefCell<LocalGrid>>>,
    buffers: [Vec<u8>; 3],
    tracker: RepaintTracker,
    #[cfg(feature = "geometry-census")]
    local_repair: Option<retinue_render::damage::Region>,
}

impl Trial {
    // Replay the frozen child once outside the timing loop to identify changes
    // in native work independently of process scheduling.
    fn frozen_work(&mut self) -> Option<serde_json::Value> {
        let grid = self.grid.as_ref()?.borrow();
        let packet = grid.packet.as_ref()?;
        let (w, h) = self.shell.pages.ui.size;
        let mut p = Painter::with_threads(
            &mut self.buffers[0][..(w * h * 4) as usize],
            w,
            h,
            NonZeroU32::MIN,
        )
        .unwrap();
        let stats = p.viewport(grid.clip, (0, grid.dy), |p| packet.draw(p));
        p.finish().unwrap();
        Some(
            serde_json::json!({"commands":stats.replayed_commands,"blits":stats.blits,
            "fragment_blits":stats.fragment_blits,"freezes":grid.freezes}),
        )
    }

    fn scene_damage(
        &mut self,
        offset: u32,
        previous: &mut Option<retinue_render::surface::Snapshot>,
    ) -> Damage {
        self.page(offset);
        self.shell.prepare_paint();
        self.prepared_damage(previous)
    }

    fn prepared_damage(&self, previous: &mut Option<retinue_render::surface::Snapshot>) -> Damage {
        let scene = if let Some(grid) = &self.grid {
            let grid = grid.borrow();
            self.shell.paint_scene_with(|scene| {
                self.shell
                    .pages
                    .ui
                    .record_surface_with(scene, |key, scene| {
                        assert_eq!(key, CHILD);
                        scene.translated(grid.slot_translation(), |scene| {
                            grid.host.record_surface(scene)
                        });
                        true
                    })
            })
        } else {
            self.shell.paint_scene()
        };
        let damage = scene.damage_since(previous.as_ref());
        *previous = Some(scene);
        damage
    }

    fn hit(&mut self, point: (f64, f64)) -> Option<String> {
        let ui = &mut self.shell.pages.ui;
        ui.pointer_move(point);
        ui.hover.clone()
    }

    fn new(scale: f32, retained: bool) -> Self {
        Self::listing(scale, Page::Relics, true, retained)
    }

    fn listing(scale: f32, page: Page, eager: bool, fixture_child: bool) -> Self {
        let mut shell = Shell::new(Some(Snapshot::fixture()), scale).unwrap();
        shell.pages.relics_view.eager_grid = eager;
        shell.pages.mastery_view.eager_grid = eager;
        shell.pages.fixture();
        shell.pages.activity = serde_json::json!({});
        let listing = match page {
            Page::Relics => &mut shell.pages.relics,
            Page::Mastery => &mut shell.pages.mastery,
            _ => unreachable!(),
        };
        let base = listing.items[0].clone();
        listing.items = (0..120)
            .map(|i| {
                let mut item = base.clone();
                item.id = format!("relic-{i}");
                item.name = format!("Relic {i:03}");
                item
            })
            .collect();
        listing.reorder(&shell.pages.quotes, true);
        shell.resize(SIZE, 1.0).unwrap();
        shell.set_page(page);
        let host = shell.render_host();
        let pages = &mut shell.pages;
        let grid = fixture_child.then(|| {
            let mut grid = LocalGrid::new(pages, true);
            grid.local_inputs = Some(Default::default());
            grid.host.set_render_host(host);
            let controller = &pages.relics_view;
            let layout =
                controller.view_with_grid(&pages.ui, GridSlot(controller.model.metrics.clone()));
            pages.ui.install_compiled(layout);
            let grid = Rc::new(RefCell::new(grid));
            let child = grid.clone();
            pages.relics_view.grid_inputs = Some(Box::new(move |m, listing, ui, media| {
                let mut child = child.borrow_mut();
                child.place_at(m.clone(), listing.offset, listing.order_revision, ui, media);
                child.inputs(m)
            }));
            grid
        });
        pages.layout_center();
        let mut tracker = RepaintTracker::default();
        let buffers = std::array::from_fn(|_| {
            tracker.add_buffer();
            vec![0; (SIZE.0 * SIZE.1 * 4) as usize]
        });
        Self {
            shell,
            grid,
            buffers,
            tracker,
            #[cfg(feature = "geometry-census")]
            local_repair: None,
        }
    }

    fn relayout(&mut self) {
        self.shell.pages.layout_center();
    }

    fn page(&mut self, offset: u32) {
        let pages = &mut self.shell.pages;
        if pages.listing().offset != offset {
            pages
                .listing_mut()
                .scroll
                .to(offset as f32, false, Instant::now());
            self.relayout();
        } else if self.grid.as_ref().is_some_and(|g| {
            let g = g.borrow();
            g.freezes == 0
                || g.host.viewport_dirty()
                || g.media_revision != Some(pages.media.revision())
                || (!g.live_packet && g.packet.is_none())
        }) {
            self.relayout();
        }
    }

    fn frame(&mut self, offset: u32, slot: usize, damage: Damage) -> [u64; 3] {
        self.frame_using(offset, slot, |_| damage)
    }

    fn frame_using(
        &mut self,
        offset: u32,
        slot: usize,
        damage: impl FnOnce(&Self) -> Damage,
    ) -> [u64; 3] {
        let started = Instant::now();
        self.page(offset);
        self.shell.prepare_paint();
        let damage = damage(self);
        self.tracker.invalidate(damage);
        let plan = self.tracker.plan(slot, SIZE).unwrap();
        let prepared = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        let wake: retinue_render::Wake = Arc::new(move || {
            let _ = tx.send(());
        });
        let region = &plan.repair;
        #[cfg(feature = "geometry-census")]
        {
            self.local_repair = None;
        }
        let mut job = if let Some(grid) = &self.grid {
            let grid = grid.borrow();
            self.shell
                .prepare_packet_with(region, wake, |page_repair, wake| {
                    self.shell.pages.ui.prepare_packet_with(
                        page_repair,
                        wake,
                        |key, repair, wake| {
                            assert_eq!(key, CHILD);
                            if grid.live_packet {
                                let offset = grid.slot_translation();
                                let local = repair.local((offset.0.into(), offset.1.into()));
                                #[cfg(feature = "geometry-census")]
                                {
                                    self.local_repair = Some(local.clone());
                                }
                                return grid
                                    .host
                                    .prepare_packet(&local, wake)
                                    .map(|job| job.placed(offset));
                            }
                            Ok(PacketJob::ready(DrawPacket::placed(
                                grid.packet.clone().unwrap(),
                                grid.slot_translation(),
                            )))
                        },
                    )
                })
        } else {
            self.shell.prepare_packet(region, wake)
        }
        .unwrap();
        let packet = loop {
            if let Some(result) = job.poll() {
                break result.unwrap();
            }
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
        };
        let native = Instant::now();
        let mut p = Painter::with_threads(
            &mut self.buffers[slot],
            SIZE.0,
            SIZE.1,
            NonZeroU32::new(4).unwrap(),
        )
        .unwrap();
        p.repair(region);
        p.clear();
        p.fill_rounded_rect(0, 0, SIZE.0, SIZE.1, 0, BACKGROUND);
        packet.draw(&mut p);
        p.finish().unwrap();
        assert!(self.tracker.submitted(slot, plan.ticket));
        [
            (prepared - started).as_nanos() as u64,
            (native - prepared).as_nanos() as u64,
            native.elapsed().as_nanos() as u64,
        ]
    }
}

#[test]
fn normal_child_packets_match_full_eager_frames_across_mount_and_resize() {
    for page in [Page::Relics, Page::Mastery] {
        for scale in [1., 1.2] {
            let mut eager = Trial::listing(scale, page, true, false);
            let mut actual = Trial::listing(scale, page, false, false);
            let mut prev = None;
            for (i, offset) in [0, 4, 153, 159, 990, 8, 0, 11].into_iter().enumerate() {
                for t in [&mut eager, &mut actual] {
                    if i == 4 {
                        t.shell.resize((SIZE.0 - 40, SIZE.1 - 12), 1.).unwrap();
                    }
                    if i == 5 {
                        t.shell.resize(SIZE, 1.).unwrap();
                    }
                    if i == 6 {
                        t.shell.set_page(Page::Inventory);
                        t.shell.set_page(page);
                    }
                }
                let slot = i % 3;
                eager.frame(offset, slot, Damage::Full);
                actual.frame_using(offset, slot, |t| t.prepared_damage(&mut prev));
                let pages = &actual.shell.pages;
                assert!(match page {
                    Page::Relics => pages.relics_view.grid_inputs.is_some(),
                    _ => pages.mastery_view.grid_inputs.is_some(),
                });
                crate::presentation::tests::assert_pixels_match(
                    &actual.buffers[slot],
                    &eager.buffers[slot],
                    &format!("production {page:?} scale {scale} frame {i}"),
                );
            }
            let key = crate::presentation::listing::retained_grid::GRID;
            let initial_hits = actual.shell.pages.ui.child_packet_counts(key).unwrap()[0];
            for i in 0..12 {
                actual.frame(11, i % 3, Damage::Full);
            }
            assert!(
                actual.shell.pages.ui.child_packet_counts(key).unwrap()[0] > initial_hits,
                "stable production {page:?} packets were never reused"
            );
        }
    }
}

#[test]
fn page_owns_viewport_focus_capture_and_accessibility() {
    for scale in [1.0, 1.2] {
        let mut eager = Trial::new(scale, false);
        let mut retained = Trial::new(scale, true);
        fn access(ui: &ControlHost) -> impl PartialEq + std::fmt::Debug {
            let tree = ui.accessibility(17);
            (
                tree.focus,
                tree.nodes
                    .into_iter()
                    .map(|(id, n)| {
                        (
                            id,
                            n.role(),
                            n.label().map(str::to_owned),
                            n.bounds(),
                            n.is_disabled(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }
        for offset in [8, 12, 240, 1400, 12] {
            eager.page(offset);
            retained.page(offset);
            let a = &mut eager.shell.pages.ui;
            let b = &mut retained.shell.pages.ui;
            assert!(
                retained
                    .grid
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .host
                    .buttons
                    .is_empty()
            );
            assert_eq!(
                access(a),
                access(b),
                "page accessibility at {scale}/{offset}"
            );
            let mut crossed = false;
            for reverse in [false, true] {
                for _ in 0..=a.buttons.len() {
                    a.focus_next(reverse);
                    b.focus_next(reverse);
                    assert_eq!(a.focus, b.focus, "tab at {scale}/{offset}");
                    crossed |= b
                        .focus
                        .as_deref()
                        .is_some_and(|k| k.starts_with("list:item:"));
                }
            }
            assert!(crossed);
        }

        for t in [&mut eager, &mut retained] {
            let ui = &mut t.shell.pages.ui;
            ui.focus = None;
            for _ in 0..ui.buttons.len() {
                ui.focus_next(false);
                if ui
                    .focus
                    .as_deref()
                    .is_some_and(|k| k.starts_with("list:item:"))
                {
                    break;
                }
            }
            assert!(ui.focus.as_deref().unwrap().starts_with("list:item:"));
            t.frame(12, 0, Damage::Full);
        }
        crate::presentation::tests::assert_pixels_match(
            &retained.buffers[0],
            &eager.buffers[0],
            "page-owned grid focus ring",
        );

        let thumb = eager.shell.pages.ui.bounds("list:scroll").unwrap();
        let start = (f64::from(thumb.x + thumb.width / 2), f64::from(thumb.y + 8));
        let card = eager.shell.pages.ui.bounds("page.card").unwrap();
        let end = (f64::from(card.x + 8), f64::from(thumb.y + thumb.height / 2));
        for t in [&mut eager, &mut retained] {
            let ui = &mut t.shell.pages.ui;
            ui.pointer_move(start);
            ui.pointer_button(true);
            t.page(16);
            let ui = &mut t.shell.pages.ui;
            ui.signals.clear();
            ui.pointer_move(end);
            assert!(ui.signals.iter().any(|s| s.key == "list:scroll"));
            ui.pointer_button(false);
            ui.signals.clear();
            ui.pointer_move((end.0, end.1 + 15.0));
            assert!(ui.signals.is_empty(), "release did not end capture");
            t.shell.pages.relics.menu = Some((0, Some(0)));
            t.shell.pages.relics.menu_anchor = (70.0, 160.0);
            t.relayout();
            t.shell.pages.ui.focus_next(false);
        }
        assert_eq!(
            access(&eager.shell.pages.ui),
            access(&retained.shell.pages.ui)
        );
        assert!(
            !retained
                .shell
                .pages
                .ui
                .focus
                .as_deref()
                .is_some_and(|key| key.starts_with("list:item:"))
        );
    }
}

#[test]
fn clean_local_input_survives_placement_and_card_hover() {
    let mut page = Trial::new(1.2, true);
    page.page(8);
    let count = |t: &Trial| t.grid.as_ref().unwrap().borrow().input_captures;
    let before = count(&page);
    let first = page
        .shell
        .pages
        .ui
        .buttons
        .iter()
        .find(|b| b.key.starts_with("list:item:") && b.key.ends_with(":menu") && b.rect.height > 24)
        .unwrap();
    let point = (f64::from(first.rect.x + 6), f64::from(first.rect.y + 12));
    assert!(page.hit(point).unwrap().starts_with("list:item:"));
    page.relayout();
    assert_eq!(
        count(&page),
        before,
        "paint-only hover rebuilt input geometry"
    );
    page.page(12);
    assert_eq!(count(&page), before, "placement copied local inputs");
    page.page(1400);
    assert!(
        count(&page) > before,
        "newly mounted controls need local geometry"
    );
    let child = Rc::downgrade(page.grid.as_ref().unwrap());
    drop(page.grid.take());
    assert!(child.upgrade().is_some());
    page.shell.set_page(Page::Inventory);
    assert!(
        child.upgrade().is_none(),
        "inactive page kept the input source alive"
    );
    page.shell.set_page(Page::Relics);
    assert!(
        page.shell
            .pages
            .ui
            .buttons
            .iter()
            .any(|b| b.key.starts_with("list:item:"))
    );
}

#[test]
fn shell_packet_preserves_scroll_clipping_and_overlapping_menu() {
    for scale in [1.0, 1.2] {
        let mut eager = Trial::new(scale, false);
        let mut retained = Trial::new(scale, true);
        retained.grid.as_ref().unwrap().borrow_mut().live_packet = true;
        let mut previous = None;
        for (frame, offset) in [0, 8, 12, 120, 1400, 12, 8, 8, 8, 8, 8]
            .into_iter()
            .enumerate()
        {
            for t in [&mut eager, &mut retained] {
                if frame == 5 || frame == 7 {
                    t.shell.pages.relics.menu = (frame == 5).then_some((0, Some(0)));
                    t.shell.pages.relics.menu_anchor = (70.0, 160.0);
                    t.relayout();
                }
            }
            let slot = frame % 3;
            eager.frame(offset, slot, Damage::Full);
            let center = retained.shell.geometry.center;
            let actual_damage = retained.scene_damage(offset, &mut previous);
            if frame == 1 || frame == 2 {
                assert_ne!(
                    actual_damage,
                    Damage::Full,
                    "offset alone invalidated the entire shell"
                );
            }
            let damage = if frame < 8 {
                assert_ne!(
                    actual_damage,
                    Damage::None,
                    "missing scene change at {frame}"
                );
                actual_damage
            } else {
                assert_eq!(
                    actual_damage,
                    Damage::None,
                    "stationary viewport still dirtied at {frame}"
                );
                Damage::rect(Rect {
                    x: center.x + 34,
                    y: center.y + 99,
                    width: 180,
                    height: 100,
                })
            };
            retained.frame(offset, slot, damage);
            assert_eq!(
                eager.shell.pages.relic_window(),
                retained.shell.pages.relic_window()
            );
            assert_eq!(
                retained.shell.pages.ui.bounds("list:menu:market").is_some(),
                (5..7).contains(&frame)
            );
            crate::presentation::tests::assert_pixels_match(
                &retained.buffers[slot],
                &eager.buffers[slot],
                &format!("shell viewport {scale}, frame {frame} offset {offset}"),
            );
            let bounds = retained.grid.as_ref().unwrap().borrow().clip;
            let mut card_hit = false;
            for y in (bounds.y.saturating_sub(2)..=bounds.y + bounds.height).step_by(39) {
                for x in (0..center.width).step_by(61) {
                    let p = (f64::from(x), f64::from(y));
                    let native = eager.hit(p);
                    card_hit |= native
                        .as_deref()
                        .is_some_and(|s| s.starts_with("list:item:"));
                    assert_eq!(retained.hit(p), native, "page pointer {p:?} frame {frame}");
                }
            }
            assert!(card_hit || (5..7).contains(&frame));
            eager.hit((-1.0, -1.0));
            retained.hit((-1.0, -1.0));
        }
        // Deliver a real grid-requested image to both owners after a packet is already resident.
        let grid = retained.grid.as_ref().unwrap().borrow();
        let reqs = grid.host.requests(&retained.shell.pages.media);
        let request = reqs
            .iter()
            .find(|r| matches!(r.source, retinue_render::thumbnails::Source::Embedded(_)))
            .expect("grid requests a bundled icon");
        let mut thumbs = retinue_render::thumbnails::Thumbnails::new(2 << 20, 2 << 20);
        let image = [(request.key, thumbs.prepare(request).unwrap())];
        let before = retained.buffers[1].clone();
        let n = grid.freezes;
        drop(grid);
        eager.shell.pages.images(&image);
        let pages = &mut retained.shell.pages;
        assert!(pages.image_requests().iter().any(|r| r.key == request.key));
        pages.images(&image);
        eager.frame(8, 1, Damage::Full);
        let damage = retained.scene_damage(8, &mut previous);
        assert_ne!(
            damage,
            Damage::None,
            "image arrival did not invalidate the viewport"
        );
        assert_ne!(
            damage,
            Damage::Full,
            "localized image arrival invalidated the entire shell"
        );
        retained.frame(8, 1, damage);
        assert!(retained.grid.as_ref().unwrap().borrow().freezes > n);
        assert!(
            retained.buffers[1] != before,
            "late image did not reach pixels"
        );
        crate::presentation::tests::assert_pixels_match(
            &retained.buffers[1],
            &eager.buffers[1],
            "late shell image",
        );
    }
}

#[test]
#[ignore = "full shell packet timing for virtual viewport placement"]
fn shell_viewport_trial() {
    use serde_json::json;
    let mut results = Vec::new();
    let reverse = usize::from(std::env::var_os("GUI_BENCH_REVERSE").is_some());
    let compare_packets = std::env::var_os("GUI_BENCH_COMPARE_PACKETS").is_some();
    let production = std::env::var_os("GUI_BENCH_PRODUCTION").is_some();
    let page = match std::env::var("GUI_BENCH_PAGE").as_deref() {
        Ok("mastery") if production => Page::Mastery,
        Err(_) | Ok("relics") => Page::Relics,
        _ => panic!("unsupported viewport trial page"),
    };
    let membership = |t: &Trial| match page {
        Page::Mastery => t.shell.pages.mastery_view.model.cells.order(),
        _ => t.shell.pages.relics_view.model.cells.order(),
    };
    assert!(!(compare_packets && production));
    let selected = std::env::var("GUI_BENCH_SCENARIOS").ok();
    for scale in [1.0, 1.2] {
        let (toolbar, card, hover) = {
            let mut fixture = Trial::listing(scale, page, true, false);
            fixture.page(8);
            let shell = &fixture.shell;
            let viewport = shell.capture_bounds("page.grid").unwrap();
            let toolbar = shell.capture_bounds("page.toolbar").unwrap();
            let card = shell
                .capture_bounds("page.card")
                .unwrap()
                .intersection(viewport);
            assert!(toolbar.intersection(viewport).is_empty() && !card.is_empty());
            let points: Vec<_> = shell
                .pages
                .ui
                .buttons
                .iter()
                .filter(|b| {
                    b.key.starts_with("list:item:")
                        && b.key.ends_with(":menu")
                        && b.rect.height > 24
                })
                .map(|b| (f64::from(b.rect.x + 6), f64::from(b.rect.y + 12)))
                .collect();
            let mut hits = std::collections::HashSet::new();
            let mut points = points.into_iter().filter(|&point| {
                fixture
                    .hit(point)
                    .is_some_and(|id| id.starts_with("list:item:") && hits.insert(id))
            });
            (
                toolbar,
                card,
                [points.next().unwrap(), points.next().unwrap()],
            )
        };
        for scenario in [
            "entry",
            "repaint-aa",
            "repaint",
            "repaint-primed",
            "repaint-live",
            "toolbar-live",
            "card-live",
            "offset",
            "offset-live",
            "crossings",
            "crossings-live",
            "repaint-reuse",
            "toolbar-reuse",
            "card-reuse",
            "offset-reuse",
            "crossings-reuse",
            "offset-damage-reuse",
            "crossings-damage-reuse",
            "hover-damage-reuse",
        ] {
            let scene_damage = scenario.contains("-damage-");
            if selected
                .as_ref()
                .is_some_and(|s| !s.split(',').any(|name| scenario == name))
            {
                continue;
            }
            if (!production && scenario == "entry")
                || (compare_packets && !scenario.ends_with("-reuse"))
            {
                continue;
            }
            let retained = scenario != "repaint-aa";
            let candidate = || {
                if production {
                    Trial::listing(scale, page, !retained, false)
                } else {
                    Trial::new(scale, retained)
                }
            };
            let baseline = || Trial::listing(scale, page, true, compare_packets);
            let mut pair = if reverse == 0 {
                [baseline(), candidate()]
            } else {
                let other = candidate();
                [baseline(), other]
            };
            if compare_packets {
                let mut grid = pair[0].grid.as_ref().unwrap().borrow_mut();
                grid.live_packet = true;
                grid.host.override_packet_reuse(false);
            }
            let mut series = [Vec::new(), Vec::new()];
            let mut snapshots = [None, None];
            if !production && (scenario.ends_with("-live") || scenario.ends_with("-reuse")) {
                let mut grid = pair[1].grid.as_ref().unwrap().borrow_mut();
                grid.live_packet = true;
                if scenario.ends_with("-live") {
                    grid.host.override_packet_reuse(false);
                }
            }
            // The toolbar gives the child empty repair demand; one card gives
            // it partial demand. Both hosts receive the same physical region.
            let center = pair[0].shell.geometry.center;
            let repair = match scenario {
                "toolbar-live" | "card-live" | "toolbar-reuse" | "card-reuse" => {
                    for t in &mut pair {
                        t.page(8);
                    }
                    if scenario.starts_with("toolbar-") {
                        toolbar
                    } else {
                        card
                    }
                }
                _ => center,
            };
            for i in 0usize..76 {
                let offset = sample_offset(scenario, i);
                let slot = i % 3;
                for k in if (i + reverse) % 2 == 0 {
                    [0, 1]
                } else {
                    [1, 0]
                } {
                    let t = &mut pair[k];
                    let members = membership(t);
                    let damage = if scenario == "entry" {
                        Damage::Full
                    } else {
                        Damage::rect(repair)
                    };
                    if i < 12
                        && scenario == "repaint-primed"
                        && let Some(grid) = &mut t.grid
                    {
                        grid.borrow_mut().packet = None;
                    }
                    let input_update = if scenario == "entry" {
                        t.shell.set_page(Page::Inventory);
                        let start = Instant::now();
                        t.shell.set_page(page);
                        start.elapsed().as_nanos() as u64
                    } else if scenario == "hover-damage-reuse" {
                        let start = Instant::now();
                        assert!(t.shell.pages.ui.pointer_move(hover[i % 2]));
                        t.relayout();
                        start.elapsed().as_nanos() as u64
                    } else {
                        0
                    };
                    let [prep, packet, native] = if scene_damage {
                        t.frame_using(offset, slot, |t| t.prepared_damage(&mut snapshots[k]))
                    } else {
                        t.frame(offset, slot, damage)
                    };
                    let prep = prep + input_update;
                    if i >= 12 {
                        series[k].push([
                            prep + packet + native,
                            u64::from(members != membership(t)),
                            prep,
                            packet,
                            native,
                        ]);
                    }
                }
            }
            let frozen_work = pair[1].frozen_work();
            let baseline = if compare_packets { "live" } else { "eager" };
            let reuse_counts = pair[1]
                .grid
                .as_ref()
                .and_then(|g| g.borrow().host.leaf_packet_counts())
                .or_else(|| pair[1].shell.pages.ui.child_packet_counts(crate::presentation::listing::retained_grid::GRID))
                .map(|[hits, builds, invalidations]| json!({"hits":hits,"builds":builds,"invalidations":invalidations}));
            results.push(json!({"scale":scale,"scenario":scenario,"target_px":SIZE,"repair_px":(!scene_damage).then_some([repair.x,repair.y,repair.width,repair.height]),"frozen_child_work":frozen_work,
                "page":format!("{page:?}"),"packet_reuse":reuse_counts,"mounted":membership(&pair[0]).keys().len(),"grid_columns":pair[0].shell.pages.listing().grid.columns,
                "columns":["through_native_finish_ns","membership_changed","update_record_ns","packet_ns","native_ns"],(baseline):series[0],"retained":series[1]}));
        }
    }
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&results).unwrap(),
    )
    .unwrap();
}
