use super::*;
use retinue_render::{DrawPacket, damage::RepaintTracker};
use std::num::NonZeroU32;
use std::sync::Arc;

mod shell_trial;

struct Run {
    pages: Pages,
    grid: LocalGrid,
    history: RepaintTracker,
    pixels: [Vec<u8>; 3],
}

struct LocalGrid {
    host: ControlHost,
    model: Model,
    retained: bool,
    band_start: u32,
    clip: Rect,
    dy: i32,
    packet: Option<Arc<DrawPacket>>,
    live_packet: bool,
    media_revision: Option<u64>,
    freezes: usize,
    keyboard: bool,
    // None in the isolated child-input oracle; the whole shell routes on the page.
    local_inputs: Option<retinue_ui::input::LocalInputGeometry>,
    input_revision: Option<(u64, u64, KeyedOrder)>,
    input_captures: usize,
}

impl Run {
    fn new(scale: f32, retained: bool) -> Self {
        let mut pages = pages(scale, 1);
        pages.configure((1000, 700), (400, 700), scale);
        let base = pages.relics.items[0].clone();
        pages.relics.items = (0..120)
            .map(|i| {
                let mut item = base.clone();
                item.id = format!("relic-{i}");
                item.name = format!("Relic {i:03}");
                item
            })
            .collect();
        pages.relics.reorder(&pages.quotes, true);
        pages.set_page(Page::Relics);
        pages.layout_center();
        let grid = LocalGrid::new(&pages, retained);
        let size = pages.ui.size;
        let mut history = RepaintTracker::default();
        let pixels = std::array::from_fn(|_| {
            history.add_buffer();
            vec![0; size.0 as usize * size.1 as usize * 4]
        });
        Self {
            pages,
            grid,
            history,
            pixels,
        }
    }

    fn place(&mut self, now: Instant) {
        let pages = &mut self.pages;
        pages.ui.hover.clone_from(&self.grid.host.hover);
        pages.relics_view.sync(
            &pages.ui,
            &mut pages.relics,
            &pages.quotes,
            &pages.quote_pending,
            &pages.preferences,
            now,
        );
        self.grid.place(pages);
    }
}

impl LocalGrid {
    fn slot_translation(&self) -> (i32, i32) {
        (-(self.clip.x as i32), self.dy - self.clip.y as i32)
    }

    fn new(pages: &Pages, retained: bool) -> Self {
        let model = Model {
            metrics: State::new(pages.relics_view.model.metrics.get()),
            offset: State::new(0.0),
            ..pages.relics_view.model.clone()
        };
        let mut host = ControlHost::new().unwrap();
        host.install_compiled_with_images(
            grid_view::GridView {
                model: model.clone(),
                fonts: Fonts::new(&host),
            },
            pages.ui.image_store(),
        );
        Self {
            host,
            model,
            retained,
            band_start: 0,
            clip: Rect::default(),
            dy: 0,
            packet: None,
            live_packet: false,
            media_revision: None,
            freezes: 0,
            keyboard: false,
            local_inputs: None,
            input_revision: None,
            input_captures: 0,
        }
    }

    fn place(&mut self, pages: &Pages) {
        self.place_at(
            pages.relics_view.model.metrics.get(),
            pages.relics.offset,
            pages.relics.order_revision,
            &pages.ui,
            &pages.media,
        );
    }

    fn place_at(
        &mut self,
        mut m: Metrics,
        offset: u32,
        data_revision: u64,
        ui: &ControlHost,
        media: &Media,
    ) {
        self.clip = m.pixel_rect(m.viewport());
        let mut size = ui.size;
        let local_offset = if self.retained && !self.keyboard {
            // A bounded coordinate window, not a backing image or extra mounted rows.
            if offset < self.band_start || offset - self.band_start > self.clip.height {
                self.band_start = offset.saturating_sub(self.clip.height / 2);
            }
            size.1 += self.clip.height;
            m.height = size.1 as f32 / m.scale;
            self.band_start
        } else {
            offset
        };
        self.dy = local_offset as i32 - offset as i32;
        self.model.offset.set(local_offset as f32 / m.scale);
        self.model.metrics.set(m);
        let revision = media.revision();
        if self.media_revision == Some(revision)
            && (self.live_packet || self.packet.is_some())
            && self
                .input_revision
                .as_ref()
                .is_none_or(|(_, r, _)| *r == data_revision)
            && !self.host.viewport_dirty()
        {
            return;
        }
        self.host.begin(size, ui.scale);
        assert!(self.host.resolve_view(FrameTime::now()));
        if let Some(inputs) = &mut self.local_inputs {
            let current = (
                self.host.input_geometry_revision(),
                data_revision,
                self.model.cells.order(),
            );
            if self.input_revision.as_ref() != Some(&current) {
                let mut keys = Vec::new();
                for &id in current.2.keys() {
                    let cell = self.model.cells.item(id).unwrap();
                    keys.extend(relics::input_keys(id, &cell.read()));
                }
                *inputs = keys
                    .into_iter()
                    .filter_map(|key| self.host.view_geometry(key).map(|node| (key, node)))
                    .collect();
                self.input_revision = Some(current);
                self.input_captures += 1;
            }
        } else {
            for &id in self.model.cells.order().keys() {
                relics::bind(&mut self.host, id, &self.model.cells.get(id).unwrap());
            }
        }
        self.host.finish();
        let domain = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        if self.live_packet {
            self.host.prepare_paint(media);
        } else {
            self.packet = Some(Arc::new(self.host.freeze_viewport(domain, media)));
        }
        self.media_revision = Some(revision);
        self.freezes += 1;
    }

    fn inputs(&self, m: &Metrics) -> retinue_ui::input::PlacedInputGeometry {
        self.local_inputs
            .as_ref()
            .unwrap()
            .placed((0, self.dy), m.scale, self.clip, m.viewport())
    }

    fn point(&mut self, p: (f64, f64)) {
        let local = if self.clip.contains(p.0, p.1) {
            (p.0, p.1 - f64::from(self.dy))
        } else {
            (-1.0, -1.0)
        };
        self.host.pointer_move(local);
    }
}

impl Run {
    fn update(&mut self, offset: u32, p: (f64, f64)) {
        let now = Instant::now();
        self.pages.relics.scroll.to(offset as f32, false, now);
        self.place(now);
        self.grid.point(p);
        if self.pages.ui.hover != self.grid.host.hover {
            self.place(now);
        }
    }

    fn tab(&mut self, reverse: bool) {
        // The local band includes partially hidden controls. Only the real viewport
        // determines tab stops and focus-ring placement.
        self.grid.keyboard = true;
        self.place(Instant::now());
        self.grid.host.focus_next(reverse);
        self.grid.packet = None;
        self.place(Instant::now());
    }

    fn draw(&mut self, slot: usize, damage: Damage) -> retinue_render::PaintStats {
        let size = self.pages.ui.size;
        self.history.invalidate(damage);
        let plan = self.history.plan(slot, size).unwrap();
        let mut p = Painter::with_threads(
            &mut self.pixels[slot],
            size.0,
            size.1,
            NonZeroU32::new(4).unwrap(),
        )
        .unwrap();
        p.repair(&plan.repair);
        p.clear();
        p.fill_rounded_rect(0, 0, size.0, size.1, 0, Color::rgb(0x191925).bgra());
        let stats = p.viewport(self.grid.clip, (0, self.grid.dy), |p| {
            self.grid.packet.as_ref().unwrap().draw(p)
        });
        p.finish().unwrap();
        assert!(self.history.submitted(slot, plan.ticket));
        stats
    }
}

fn verify(scale: f32) {
    let mut refview = Run::new(scale, false);
    let mut test = Run::new(scale, true);
    let mut seen_hit = false;
    let mut changed_under_pointer = false;
    let mut last = None;
    let point = (85.0 * f64::from(scale), 150.0 * f64::from(scale));
    for (i, offset) in [8, 12, 8, 72, 125, 136, 520, 1400, 1404, 520, 0, 4, 4, 4]
        .into_iter()
        .enumerate()
    {
        for view in [&mut refview, &mut test] {
            view.update(offset, point);
        }
        assert_eq!(test.grid.host.hover, refview.grid.host.hover);
        seen_hit |= test.grid.host.hover.is_some();
        changed_under_pointer |= i > 0 && test.grid.host.hover != last;
        last.clone_from(&test.grid.host.hover);
        if i == 1 || i == 4 {
            refview.grid.host.pointer_button(i == 1);
            test.grid.host.pointer_button(i == 1);
        }
        assert_eq!(test.grid.host.focus, refview.grid.host.focus);
        assert!(
            test.grid
                .host
                .signals
                .iter()
                .map(|s| (&s.key, &s.value))
                .eq(refview.grid.host.signals.iter().map(|s| (&s.key, &s.value)))
        );
        let slot = i % 3;
        let damage = if i >= 12 {
            Damage::rect(Rect {
                x: 70,
                y: 100,
                width: 77,
                height: 68,
            })
        } else {
            Damage::rect(test.grid.clip)
        };
        refview.draw(slot, Damage::Full);
        test.draw(slot, damage);
        crate::presentation::tests::assert_pixels_match(
            &test.pixels[slot],
            &refview.pixels[slot],
            &format!("viewport placement, scale {scale}, offset {offset}, frame {i}"),
        );
        let clip = test.grid.clip;
        for y in (clip.y.saturating_sub(1)..=clip.y + clip.height)
            .step_by(17)
            .chain([clip.y, clip.y + clip.height - 1, clip.y + clip.height])
        {
            for x in (0..test.pages.ui.size.0).step_by(39) {
                let pos = (f64::from(x), f64::from(y));
                test.grid.point(pos);
                refview.grid.point(pos);
                assert_eq!(
                    test.grid.host.hover, refview.grid.host.hover,
                    "offset {offset}, point {pos:?}"
                );
            }
        }
        test.grid.point(point);
        refview.grid.point(point);
    }
    assert!(seen_hit && changed_under_pointer);
    assert!(test.grid.freezes < refview.grid.freezes);

    // Editing a mounted row must invalidate its packet even with a fixed offset.
    let before = test.pixels[1].clone();
    for view in [&mut test, &mut refview] {
        let changed = view.pages.relics.visible[1];
        view.pages
            .relics
            .edit_fixture_rows(|items| items[changed].name = "Updated price fixture".into());
        let n = view.grid.freezes;
        view.update(4, point);
        assert!(view.grid.freezes > n);
        view.draw(1, Damage::rect(view.grid.clip));
    }
    assert!(
        before != test.pixels[1],
        "mounted row edit did not reach pixels"
    );
    crate::presentation::tests::assert_pixels_match(
        &test.pixels[1],
        &refview.pixels[1],
        "content edit",
    );
    let clip = refview.grid.clip;
    let (click, expected) = (clip.y..clip.y + clip.height)
        .step_by(16)
        .find_map(|y| {
            (clip.x..clip.x + clip.width).step_by(16).find_map(|x| {
                let p = (f64::from(x), f64::from(y));
                refview.grid.point(p);
                let hit = refview.grid.host.hover.clone()?;
                refview.grid.point((p.0, p.1 + 3.0));
                (refview.grid.host.hover.as_ref() == Some(&hit)).then_some((p, hit))
            })
        })
        .expect("interior of a mounted card hit target");
    for view in [&mut test, &mut refview] {
        view.update(4, click);
        view.grid.host.pointer_button(true);
        assert_eq!(view.grid.host.focus.as_ref(), Some(&expected));
        view.update(6, click);
        let count = view.grid.host.signals.len();
        view.grid.host.pointer_button(false);
        assert_eq!(view.grid.host.signals.len(), count + 1);
        assert_eq!(view.grid.host.signals.last().unwrap().key, expected);
    }
    for rev in [false, false, true] {
        for view in [&mut test, &mut refview] {
            view.tab(rev);
            view.draw(2, Damage::rect(view.grid.clip));
        }
        assert!(test.grid.host.focus.is_some());
        assert_eq!(test.grid.host.focus, refview.grid.host.focus);
        crate::presentation::tests::assert_pixels_match(
            &test.pixels[2],
            &refview.pixels[2],
            "keyboard focus",
        );
    }
}

#[test]
fn viewport_placement_matches_eager_pixels_and_pointer() {
    for scale in [1.0, 1.2] {
        verify(scale);
    }
}

#[test]
#[ignore = "headless virtual viewport propagation through native completion"]
fn scroll_viewport_trial() {
    use serde_json::json;
    let mut results = Vec::new();
    let reverse = usize::from(std::env::var_os("GUI_BENCH_REVERSE").is_some());
    for scale in [1.0, 1.2] {
        verify(scale);
        for scenario in ["repaint-aa", "repaint", "offset", "crossings", "hover"] {
            let retained = scenario != "repaint-aa";
            let mut views = if reverse == 0 {
                [Run::new(scale, false), Run::new(scale, retained)]
            } else {
                let retained = Run::new(scale, retained);
                [Run::new(scale, false), retained]
            };
            let mut series = [Vec::new(), Vec::new()];
            for i in 0..76 {
                let offset = match scenario {
                    "repaint" | "repaint-aa" => 8,
                    "offset" => [8, 12][i % 2],
                    "crossings" => 8 + (i % 24).abs_diff(12) as u32 * 24,
                    _ => 8 + (i % 32).abs_diff(16) as u32 * 4,
                };
                for mode in if (i + reverse) % 2 == 0 {
                    [0, 1]
                } else {
                    [1, 0]
                } {
                    let v = &mut views[mode];
                    let window = v.pages.relic_window();
                    let start = Instant::now();
                    v.update(
                        offset,
                        if scenario == "hover" {
                            (85.0 * scale as f64, 150.0 * scale as f64)
                        } else {
                            (-1.0, -1.0)
                        },
                    );
                    let prepared = start.elapsed();
                    let stats = v.draw(i % 3, Damage::rect(v.grid.clip));
                    let total = start.elapsed();
                    if i >= 12 {
                        series[mode].push([
                            prepared.as_nanos() as u64,
                            total.as_nanos() as u64,
                            u64::from(window != v.pages.relic_window()),
                            v.pages.relic_window().len() as u64,
                            stats.replayed_commands as u64,
                            stats.culled_commands as u64,
                            stats.blits as u64,
                        ]);
                    }
                }
            }
            results.push(json!({"scale":scale,"scenario":scenario,"target_px":views[0].pages.ui.size,"grid_columns":views[0].pages.relics.grid.columns,"viewport_px":[views[0].grid.clip.width,views[0].grid.clip.height],"columns":["update_and_input_ns","through_native_finish_ns","membership_changed","mounted","replayed_commands","culled_commands","blits"],
                "eager":series[0],"retained":series[1],"freezes_including_warmup":[views[0].grid.freezes,views[1].grid.freezes]}));
        }
    }
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&results).unwrap(),
    )
    .unwrap();
}
