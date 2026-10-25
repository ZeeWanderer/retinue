use super::*;

pub(super) fn view() -> Foundry {
    let mut view = Foundry::new(Default::default()).unwrap();
    view.resize((1300, 900), 1.25);
    view.update(model::fixture());
    view
}

#[test]
fn composition_keeps_only_visible_cards_and_stable_item_identities() {
    let mut view = view();
    for scale in [1.0, 1.25, 2.0] {
        view.resize((1300, 900), scale);
        for offset in [0, 53, 800] {
            view.offset = offset;
            let scene = view.resolved_composition();
            assert_eq!(
                scene
                    .scene
                    .geometry()
                    .iter()
                    .filter(|node| node.key.namespace == "foundry.card")
                    .count(),
                view.visible(0).len()
            );
            for (action, r) in scene.controls(true) {
                if matches!(action, Action::Favorite(_)) {
                    assert_eq!(
                        scene.hit(((r.x + r.width / 2) as f64, (r.y + r.height / 2) as f64)),
                        Some(action)
                    );
                }
                assert_eq!(r.intersection(view.viewport), r);
            }
        }
    }
    view.offset = 0;
    let before = view
        .resolved_composition()
        .scene
        .geometry()
        .iter()
        .filter(|node| node.key.namespace == "foundry.card")
        .map(|node| node.key)
        .collect::<Vec<_>>();
    let count = view.visible(0).len();
    view.filtered[..count].reverse();
    view.layout_revision += 1;
    let after = view
        .resolved_composition()
        .scene
        .geometry()
        .iter()
        .filter(|node| node.key.namespace == "foundry.card")
        .map(|node| node.key)
        .collect::<Vec<_>>();
    assert_eq!(before.into_iter().rev().collect::<Vec<_>>(), after);
}

#[test]
fn scrolling_within_visible_rows_reuses_layout_and_recording() {
    let mut view = view();
    view.offset = 51;
    let keys = view
        .resolved_composition()
        .scene
        .geometry()
        .iter()
        .map(|node| node.key)
        .collect::<Vec<_>>();
    view.offset = 52;
    let composition = view.resolved_composition();
    assert_eq!(composition.scene.stats.layout_computations, 0);
    assert_eq!(composition.scene.stats.recordings, 0);
    assert_eq!(
        composition
            .scene
            .geometry()
            .iter()
            .map(|node| node.key)
            .collect::<Vec<_>>(),
        keys
    );
}

#[test]
fn favorites_use_shared_canonical_ids_across_refreshes() {
    let mut view = view();
    let id = view.snapshot.items[0].id.clone();
    view.activate(Action::Favorite(0));
    assert!(view.preferences.is_favorite(&id));
    let mut refreshed = model::fixture();
    refreshed.items.reverse();
    view.update(refreshed);
    assert!(view.preferences.is_favorite(&id));
    let index = view
        .snapshot
        .items
        .iter()
        .position(|card| card.id == id)
        .unwrap();
    view.activate(Action::Favorite(index));
    assert!(!view.preferences.is_favorite(&id));
}

#[test]
fn cached_component_glow_matches_vector_paint_at_all_scales() {
    for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
        let mut view = view();
        view.resize((1300, 900), scale);
        let glow = view.component_glow.as_ref().unwrap();
        let diameter = view.px(39.0);
        assert_eq!(glow.dimensions(), (diameter, diameter));
        let r = Rect {
            x: 3,
            y: 5,
            width: diameter,
            height: diameter,
        };
        let size = (diameter + 8, diameter + 10);
        let mut pixels = [
            vec![0; (size.0 * size.1 * 4) as usize],
            vec![0; (size.0 * size.1 * 4) as usize],
        ];
        for (i, buffer) in pixels.iter_mut().enumerate() {
            let mut p = Painter::new(buffer, size.0, size.1).unwrap();
            p.fill_rounded_rect(0, 0, size.0, size.1, 0, CARD);
            p.fill_rounded_rect(r.x, r.y, r.width, r.height, diameter / 2, COMPONENT_READY);
            if i == 0 {
                p.draw_image(glow, r.x, r.y, r.width, r.height);
            } else {
                p.inset_shadow(r, diameter / 2, 4.0 * scale, scale, COMPONENT_GLOW);
            }
            p.finish().unwrap();
        }
        let worst = pixels[0]
            .iter()
            .zip(&pixels[1])
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 3, "scale {scale}: compositing difference {worst}");
    }
}

#[test]
fn component_badges_follow_aleca_lower_corner_anchors_at_all_scales() {
    for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
        let mut view = view();
        view.resize((1300, 900), scale);
        for part in view.parts(5) {
            let relic = view.relic_badge(part);
            let count = view.count_badge(part, "3", true);
            assert_eq!(relic.width, view.px(21.0));
            assert_eq!(relic.x + view.px(2.0), part.x);
            assert!(relic.y > part.y + part.height / 2);
            assert!(relic.x + relic.width <= count.x);
            assert_eq!(count.x + count.width, part.x + part.width + view.px(3.0));
            assert!(count.y > part.y + part.height / 2);
            assert!(view.count_badge(part, "1234567", true).width <= view.px(27.0));
            assert!(view.count_badge(part, "1234567", false).width > view.px(39.0));
            for amount in 1..200 {
                let text = amount.to_string();
                let count = view.count_badge(part, &text, false);
                assert!(
                    (count.width - view.px(8.0)) as f32
                        >= text_width(&view.regular, &text, 12.5 * scale),
                    "scale {scale}, count {text}"
                );
            }
        }
    }
}

#[test]
fn card_palette_and_hover_damage_match_full_repaint() {
    use retinue_ui::painter::render_thread_budget;
    for scale in [1.0, 1.25, 2.0] {
        let mut view = view();
        view.resize((1300, 900), scale);
        let size = view.size;
        let paint = |view: &Foundry, pixels: &mut [u8], rect: Rect| {
            let mut p =
                Painter::with_threads(pixels, size.0, size.1, render_thread_budget(4)).unwrap();
            p.clip_to_rect(rect);
            p.fill_rounded_rect(rect.x, rect.y, rect.width, rect.height, 0, PANEL);
            view.paint(&mut p, Default::default());
            p.finish().unwrap();
        };
        let full = Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        paint(&view, &mut pixels, full);
        for i in [0, 1] {
            let card = view.screen_card(i);
            let x = card.x + view.px(20.0);
            let y = card.y + view.px(55.0);
            let index = ((y * size.0 + x) * 4) as usize;
            let expected = if i == 0 { [57, 63, 39, 255] } else { CARD };
            for (got, want) in pixels[index..index + 4].iter().zip(expected) {
                assert!(
                    got.abs_diff(want) <= 1,
                    "scale {scale}, item {i}, {got} != {want}"
                );
            }
            if i == 1 {
                let edge = (((card.y + card.height / 2) * size.0 + card.x) * 4) as usize;
                assert_eq!(&pixels[edge..edge + 4], &CARD, "No idle outline");
            }
        }
        for i in [Some(0), Some(1), None] {
            let pointer = i
                .map(|i| {
                    let r = view.screen_card(i);
                    ((r.x + view.px(20.0)) as f64, (r.y + view.px(55.0)) as f64)
                })
                .unwrap_or((-1.0, -1.0));
            let damage = view.pointer_move(pointer);
            assert!(!matches!(damage, Damage::Full | Damage::None));
            paint(&view, &mut pixels, damage.bounds(size).unwrap());
            let mut expected = vec![0; pixels.len()];
            paint(&view, &mut expected, full);
            assert_eq!(pixels.iter().zip(&expected).position(|(a, b)| a != b), None);
        }
    }
}

#[test]
fn refresh_and_reflow_preserve_visible_item_and_scroll() {
    let mut view = view();
    view.jump(970.0);
    let anchor = view.anchor().unwrap();
    let mut snapshot = model::fixture();
    snapshot.items[0].owned = !snapshot.items[0].owned;
    snapshot.items.insert(
        0,
        Card {
            id: "new".into(),
            name: "New item".into(),
            group: "warframe".into(),
            ..Default::default()
        },
    );
    view.update(snapshot);
    assert!(view.offset > 0);
    assert!(
        view.visible(0)
            .any(|i| view.snapshot.items[view.filtered[i]].id == anchor.0)
    );
    let anchor = view.anchor().unwrap();
    view.resize((1000, 900), 1.25);
    assert!(
        view.visible(0)
            .any(|i| view.snapshot.items[view.filtered[i]].id == anchor.0)
    );
}

#[test]
fn search_filters_locally_without_changing_snapshot_and_empty_grid_stops_scrolling() {
    let mut view = view();
    view.set_search("neuroptics");
    assert_eq!(view.count(), 120);
    view.set_search("does not exist");
    assert_eq!(view.count(), 0);
    assert_eq!(view.maximum(), 0.0);
    assert_eq!(view.snapshot.items.len(), 240);
    view.set_search("");
    view.set_category(2);
    assert_eq!(view.count(), 120);
}

#[test]
fn category_search_and_scrollbar_bounds_fit_at_all_scales() {
    for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
        for width in [520, 1000, 1800] {
            let mut view = view();
            view.resize((width, 900), scale);
            for (_, rect) in view.controls() {
                assert!(rect.x + rect.width <= width, "{scale} {width} {rect:?}");
            }
            assert!(view.visible(1).len() < 60);
            view.jump(f32::MAX);
            assert!(view.offset as f32 <= view.maximum() + 1.0);
            let thumb = view.scrollbar();
            assert!(thumb.y + thumb.height <= 900);
        }
    }
}

#[test]
fn category_motion_moves_both_capsules_without_rewrapping_or_jumping_on_retarget() {
    for scale in [1.0, 1.25, 2.0] {
        for width in [520, 1000, 1800] {
            let mut view = view();
            view.resize((width, 900), scale);
            let top = view.viewport.y;
            let now = Instant::now();
            let initial = view.controls();
            view.select_category(3, now);
            assert_eq!(view.controls(), initial);
            view.tick(now + Duration::from_millis(85));
            assert_eq!(view.category_motion.weights[1], 0.5);
            assert_eq!(view.category_motion.weights[3], 0.5);
            let middle = view.controls();
            view.select_category(7, now + Duration::from_millis(85));
            assert_eq!(view.controls(), middle);
            for ms in [90, 130, 180, 255] {
                view.tick(now + Duration::from_millis(ms));
                let controls = view.controls();
                assert_eq!(view.viewport.y, top);
                assert!((view.category_motion.weights.iter().sum::<f32>() - 1.0).abs() < 0.0001);
                for (action, rect) in &controls {
                    assert!(rect.x + rect.width <= width);
                    assert_eq!(
                        view.hit((
                            (rect.x + rect.width / 2) as f64,
                            (rect.y + rect.height / 2) as f64
                        )),
                        Some(*action)
                    );
                }
                for pair in controls[..8].windows(2) {
                    if pair[0].1.y == pair[1].1.y {
                        assert!(pair[0].1.x + pair[0].1.width < pair[1].1.x);
                    }
                }
                assert_eq!(
                    controls.iter().map(|(_, r)| r.y + r.height).max().unwrap() + view.px(30.0),
                    top
                );
            }
            assert_eq!(view.category_motion.weights[7], 1.0);
            assert!(
                view.category_motion
                    .deadline(now + Duration::from_millis(255))
                    .is_none()
            );
        }
    }
}

#[test]
fn image_progress_only_damages_cards_using_new_pixels() {
    let mut view = view();
    let asset = Asset {
        id: "progress".into(),
        source: "wfcd".into(),
        image_name: "progress.png".into(),
    };
    view.snapshot.items[0].asset = Some(asset.clone());
    view.assets(vec![Descriptor {
        id: asset.id.clone(),
        path: "unused".into(),
        digest: "11".repeat(32),
    }]);
    let key = view.request(Some(&asset), view.px(105.0)).unwrap().key;
    let mut cache = retinue_ui::resources::ResourceCache::new(1024);
    let image = cache.insert(
        key,
        RasterImage::from_rgba(retinue_ui::image::RgbaImage::new(2, 2)).unwrap(),
    );
    let damage = view.accept_images(vec![(key, image.clone())], Vec::new());
    assert_eq!(damage.bounds(view.size), Some(view.screen_card(0)));
    assert!(matches!(
        view.accept_images(vec![(key, image)], Vec::new()),
        Damage::None
    ));
    assert!(matches!(
        view.accept_images(Vec::new(), Vec::new()),
        Damage::None
    ));
}

#[test]
fn clearing_images_releases_composed_rasters_and_rebuilds_the_view() {
    let mut view = view();
    let before = view.resolved_composition().scene.geometry().len();
    assert!(view.composition.borrow().cache_stats().live_bytes > 0);
    view.clear_images();
    assert_eq!(view.composition.borrow().cache_stats().live_bytes, 0);
    assert_eq!(view.composition.borrow().cache_stats().cached_bytes, 0);
    assert_eq!(view.resolved_composition().scene.geometry().len(), before);
    assert!(view.composition.borrow().cache_stats().live_bytes > 0);
}

#[test]
fn paint_keys_match_worker_requests_without_disk_identity() {
    let mut descriptor = Descriptor {
        id: "asset".into(),
        path: "not-read-for-key".into(),
        digest: "42".repeat(32),
    };
    for size in [35, 44, 105, 131, 210] {
        let key = descriptor.key(size).unwrap();
        let request = descriptor
            .request(size)
            .unwrap()
            .persistent("wfcd", "asset.png");
        assert_eq!(key, request.key);
        assert!(request.origin.is_some());
    }
    descriptor.digest = "invalid digest".into();
    assert!(descriptor.key(35).is_none());
    assert!(descriptor.request(35).is_none());
}

#[test]
fn animated_toolbar_damage_matches_full_paint() {
    use retinue_ui::painter::render_thread_budget;
    for scale in [1.0, 1.25, 2.0] {
        let mut view = view();
        let size = (1000, 900);
        view.resize(size, scale);
        let now = Instant::now();
        view.select_category(2, now);
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        for ms in [0, 30, 85, 140, 170] {
            let damage = view.tick(now + Duration::from_millis(ms));
            let bounds = if ms == 0 {
                Rect {
                    x: 0,
                    y: 0,
                    width: size.0,
                    height: size.1,
                }
            } else {
                damage.bounds(size).unwrap()
            };
            let mut painter =
                Painter::with_threads(&mut pixels, size.0, size.1, render_thread_budget(4))
                    .unwrap();
            painter.clip_to_rect(bounds);
            painter.fill_rounded_rect(bounds.x, bounds.y, bounds.width, bounds.height, 0, PANEL);
            view.paint(&mut painter, Default::default());
            painter.finish().unwrap();
            let mut expected = vec![0; pixels.len()];
            let mut painter =
                Painter::with_threads(&mut expected, size.0, size.1, render_thread_budget(4))
                    .unwrap();
            painter.fill_rounded_rect(0, 0, size.0, size.1, 0, PANEL);
            view.paint(&mut painter, Default::default());
            painter.finish().unwrap();
            assert_eq!(
                pixels.iter().zip(&expected).position(|(a, b)| a != b),
                None,
                "scale {scale}, time {ms}: first mismatched byte"
            );
        }
    }
}

#[test]
#[ignore = "opt-in image preparation benchmark using cached player data"]
fn foundry_image_benchmark() {
    use clap::Parser;
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let args = crate::Args::parse_from(["wfgui", "--size", "2560x1440", "--ui-scale", "125"]);
    let mut shell = crate::Shell::new(Some(crate::data::Snapshot::fixture()), 1.25).unwrap();
    shell.resize(args.size, 1.0).unwrap();
    shell.set_page(crate::Page::Foundry);
    crate::prepare_capture(&mut shell, &args).unwrap();
    let requests = shell.foundry.image_requests();
    assert!(
        requests.iter().any(|r| r.origin.is_some()),
        "benchmark needs catalog assets"
    );
    let root = tempfile::tempdir().unwrap();
    let mut rows = Vec::new();
    let mut cache = images::cache(Some(root.path().into()));
    for mode in ["cold", "restart", "ram"] {
        if mode == "restart" {
            cache = images::cache(Some(root.path().into()));
        }
        let before = cache.work;
        let started = Instant::now();
        cache.begin_batch(&requests);
        let mut prepared = Vec::new();
        let mut first = None;
        for request in &requests {
            let image = cache.prepare(request).unwrap();
            if request.origin.is_some() && first.is_none() {
                first = Some(started.elapsed().as_secs_f64() * 1000.0);
            }
            prepared.push((request, image));
        }
        let ready = started.elapsed().as_secs_f64() * 1000.0;
        for (request, image) in prepared {
            cache.persist(request, &image);
        }
        assert!(cache.take_warnings().is_empty());
        rows.push(serde_json::json!({"mode":mode,"first_catalog_image_ms":first,"all_images_ms":ready,"including_writes_ms":started.elapsed().as_secs_f64()*1000.0,"persistent_resamples":cache.work.persistent_resamples-before.persistent_resamples,"disk_hits":cache.work.disk_hits-before.disk_hits,"ram_hits":cache.work.ram_hits-before.ram_hits}));
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &serde_json::json!({"images":requests.len(),"measurements":rows}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn source_identity_change_replaces_thumbnail_request_without_scroll_jump() {
    let mut view = view();
    view.snapshot.items[0].asset = Some(Asset {
        id: "asset".into(),
        source: "wfcd".into(),
        image_name: "a.png".into(),
    });
    view.assets(vec![Descriptor {
        id: "asset".into(),
        path: "a.png".into(),
        digest: "11".repeat(32),
    }]);
    let old = view.image_requests();
    view.assets(vec![Descriptor {
        id: "asset".into(),
        path: "b.png".into(),
        digest: "22".repeat(32),
    }]);
    let new = view.image_requests();
    assert_ne!(old, new);
    assert_eq!(view.offset, 0);
}

#[test]
fn recycled_buffers_repair_scroll_and_changed_card_pixels_like_full_paint() {
    use retinue_ui::{damage::AgeRepaintTracker, painter::render_thread_budget};
    let mut view = view();
    view.resize((1000, 760), 1.25);
    let size = (1000, 760);
    let mut buffers = [
        vec![0u8; size.0 as usize * size.1 as usize * 4],
        vec![0u8; size.0 as usize * size.1 as usize * 4],
    ];
    let mut tracker = AgeRepaintTracker::default();
    for frame in 0..12 {
        if frame % 3 != 0 {
            view.wheel(31.0, false);
            tracker.invalidate(view.tick(Instant::now()));
        }
        let mut snapshot = view.snapshot.clone();
        let index = view.filtered[view.visible(0).start + view.grid.columns];
        snapshot.items[index].pending = !snapshot.items[index].pending;
        tracker.invalidate(view.update(snapshot));
        let plan = tracker.plan(if frame < 2 { 0 } else { 2 }, size).unwrap();
        let buffer = &mut buffers[frame % 2];
        let mut painter =
            Painter::with_threads(buffer, size.0, size.1, render_thread_budget(4)).unwrap();
        painter.clip_to_rect(plan.repair);
        painter.fill_rounded_rect(
            plan.repair.x,
            plan.repair.y,
            plan.repair.width,
            plan.repair.height,
            0,
            PANEL,
        );
        view.paint(&mut painter, Default::default());
        painter.finish().unwrap();
        let mut expected = vec![0u8; buffer.len()];
        let mut painter =
            Painter::with_threads(&mut expected, size.0, size.1, render_thread_budget(4)).unwrap();
        painter.fill_rounded_rect(0, 0, size.0, size.1, 0, PANEL);
        view.paint(&mut painter, Default::default());
        painter.finish().unwrap();
        let mismatch = buffer.iter().zip(&expected).position(|(a, b)| a != b);
        assert_eq!(
            mismatch, None,
            "recycled frame {frame}: first mismatched byte"
        );
        tracker.submitted();
    }
}

#[test]
fn unchanged_and_offscreen_updates_do_not_repaint_visible_cards() {
    let mut view = view();
    let snapshot = view.snapshot.clone();
    assert!(matches!(view.update(snapshot), Damage::None));
    let mut snapshot = view.snapshot.clone();
    snapshot.items[119].owned = !snapshot.items[119].owned;
    assert!(matches!(view.update(snapshot), Damage::None));
    let mut snapshot = view.snapshot.clone();
    snapshot.items[0].owned = !snapshot.items[0].owned;
    let damage = view.update(snapshot).bounds(view.size).unwrap();
    assert_eq!(damage, view.screen_card(0));
}

#[test]
fn asset_refresh_keeps_previous_pixels_until_replacement_is_ready() {
    use retinue_ui::{image, resources::ResourceCache};
    let mut view = view();
    let asset = Asset {
        id: "asset".into(),
        source: "wfcd".into(),
        image_name: "a.png".into(),
    };
    view.snapshot.items[0].asset = Some(asset.clone());
    let size = view.px(105.0);
    view.assets(vec![Descriptor {
        id: asset.id.clone(),
        path: "a.png".into(),
        digest: "11".repeat(32),
    }]);
    let old = view.request(Some(&asset), size).unwrap().key;
    let mut cache = ResourceCache::new(4096);
    let image = cache.insert(
        old,
        RasterImage::from_rgba(image::RgbaImage::new(8, 8)).unwrap(),
    );
    view.accept_images(vec![(old, image)], Vec::new());
    view.assets(vec![Descriptor {
        id: asset.id.clone(),
        path: "b.png".into(),
        digest: "22".repeat(32),
    }]);
    let new = view.request(Some(&asset), size).unwrap().key;
    view.accept_images(Vec::new(), vec!["source unavailable".into()]);
    assert!(view.displayed_image(Some(&asset), size).is_some());
    assert!(view.images.contains_key(&old));
    let image = cache.insert(
        new,
        RasterImage::from_rgba(image::RgbaImage::new(8, 8)).unwrap(),
    );
    view.accept_images(vec![(new, image)], Vec::new());
    assert!(view.displayed_image(Some(&asset), size).is_some());
    assert!(!view.images.contains_key(&old));
    assert!(view.images.contains_key(&new));
}

#[test]
fn search_caret_movement_preserves_scroll_but_edit_resets_it() {
    let mut view = view();
    view.set_search("neuroptics");
    view.jump(600.0);
    view.focus = Some(Action::Search);
    assert!(view.key(InputKey::Named(NamedKey::Home), ModifiersState::empty()));
    assert_eq!(view.offset, 600);
    assert!(view.key(InputKey::Character("a"), ModifiersState::empty()));
    assert_eq!(view.offset, 0);
}

#[test]
#[ignore = "opt-in scrolling benchmark; set GUI_BENCH_LIVE=1 for cached player data and images"]
fn foundry_scroll_benchmark() {
    use clap::Parser;
    use rustix::time::{ClockId, clock_gettime};
    let cpu_ns = |clock| {
        let time = clock_gettime(clock);
        time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
    };
    let threads = crate::benchmark::threads();
    let live = std::env::var("GUI_BENCH_LIVE").is_ok_and(|v| v == "1");
    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let mut reports = Vec::new();
    for (w, h, scale) in [
        (1920, 1080, 100),
        (2560, 1440, 100),
        (2560, 1440, 125),
        (2560, 1440, 200),
    ] {
        let mut args = vec![
            "wfgui".to_string(),
            "--size".into(),
            format!("{w}x{h}"),
            "--ui-scale".into(),
            scale.to_string(),
        ];
        if !live {
            args.push("--fixture".into());
        }
        let args = crate::Args::parse_from(args);
        let mut shell =
            crate::shell::Shell::new(Some(crate::data::Snapshot::fixture()), scale as f32 / 100.0)
                .unwrap();
        shell.resize((w, h), 1.0).unwrap();
        shell.set_page(crate::shell::Page::Foundry);
        crate::prepare_capture(&mut shell, &args).unwrap();
        let mut buffer = vec![0u32; w as usize * h as usize];
        let mut glow = shell.foundry.component_glow.take();
        for (cached, full) in [(false, false), (true, false), (true, true)] {
            if cached && shell.foundry.component_glow.is_none() {
                shell.foundry.component_glow = glow.take();
            }
            let mut samples = Vec::new();
            let mut phases = Vec::new();
            let mut actual_threads = (u32::MAX, 0);
            let mut last = None;
            for frame in 0..180 {
                let process = cpu_ns(ClockId::ProcessCPUTime);
                let caller = cpu_ns(ClockId::ThreadCPUTime);
                let start = Instant::now();
                let offset = ((frame % 120).min(120 - frame % 120)) as f32 * 2.0;
                shell.foundry.jump(offset);
                let (update_stats, prepared) = {
                    let composition = shell.foundry.resolved_composition();
                    (composition.scene.stats, composition.paint_stats())
                };
                let updated = Instant::now();
                let repair = if full || last.is_none() {
                    Rect {
                        x: 0,
                        y: 0,
                        width: w,
                        height: h,
                    }
                } else {
                    shell
                        .page_damage(Damage::rect(Rect {
                            x: 0,
                            y: shell.foundry.viewport.y,
                            width: shell.geometry.center.width,
                            height: shell.foundry.viewport.height,
                        }))
                        .bounds((w, h))
                        .unwrap()
                };
                let mut painter =
                    Painter::with_threads(bytemuck::cast_slice_mut(&mut buffer), w, h, threads)
                        .unwrap();
                let actual = painter.thread_count();
                actual_threads = (actual_threads.0.min(actual), actual_threads.1.max(actual));
                painter.clip_to_rect(repair);
                painter.fill_rounded_rect(
                    repair.x,
                    repair.y,
                    repair.width,
                    repair.height,
                    0,
                    crate::shell::BACKGROUND,
                );
                shell.paint(&mut painter);
                let submitted = Instant::now();
                painter.finish().unwrap();
                let finished = Instant::now();
                let caller = cpu_ns(ClockId::ThreadCPUTime) - caller;
                let process = cpu_ns(ClockId::ProcessCPUTime) - process;
                if frame >= 20 {
                    samples.push((finished - start).as_secs_f64() * 1000.0);
                    let composition = shell.foundry.resolved_composition();
                    phases.push((
                        [
                            (updated - start).as_secs_f64() * 1000.0,
                            (submitted - updated).as_secs_f64() * 1000.0,
                            (finished - submitted).as_secs_f64() * 1000.0,
                            caller as f64 / 1e6,
                            process as f64 / 1e6,
                        ],
                        [
                            update_stats.layout_computations,
                            update_stats.recordings,
                            update_stats.nodes,
                            prepared.prepared_rasters,
                            prepared.repaired_pixels,
                            prepared.copied_pixels,
                            composition.paint_stats().blits,
                            composition.cache_stats().live_bytes,
                        ],
                    ));
                }
                last = Some(offset);
            }
            let mut sorted = samples.clone();
            sorted.sort_by(f64::total_cmp);
            let phases: Vec<_> = phases.into_iter().map(|(time, counters)| serde_json::json!({
                "update_ms":time[0],"submit_ms":time[1],"finish_ms":time[2],
                "caller_cpu_ms":time[3],"process_cpu_ms":time[4],
                "layout_computations":counters[0],"recordings":counters[1],"nodes":counters[2],
                "prepared_rasters":counters[3],"repaired_pixels":counters[4],"copied_pixels":counters[5],"blits":counters[6],"raster_bytes":counters[7],
            })).collect();
            let stats = shell.foundry.cache_stats;
            reports.push(serde_json::json!({"size":[w,h],"scale":scale,"threads":threads.get(),"actual_threads":actual_threads,"live":live,"mode":if full{"full_window"}else{"viewport_repaint"},"cached_glow":cached,"columns":shell.foundry.grid.columns,"visible_cards":shell.foundry.visible(0).len(),"total_cards":shell.foundry.snapshot.items.len(),"prepared_images":shell.foundry.prepared_count(),"decoded_source_bytes":stats.0.live_bytes,"thumbnail_bytes":stats.1.live_bytes,"image_errors":shell.foundry.image_errors,"p50_ms":sorted[sorted.len()/2],"p95_ms":sorted[(sorted.len()-1)*95/100],"max_ms":sorted.last(),"samples_ms":samples,"phases":phases}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
