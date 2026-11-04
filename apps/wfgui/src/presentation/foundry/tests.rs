use super::*;

pub(super) fn view() -> Foundry {
    let mut view = Foundry::new(Default::default()).unwrap();
    view.resize((1300, 900), 1.25);
    view.update(model::fixture());
    view
}

fn render(view: &Foundry) -> Vec<u8> {
    let (width, height) = view.size;
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut painter = Painter::new(&mut pixels, width, height).unwrap();
    view.paint(&mut painter, Default::default());
    painter.finish().unwrap();
    pixels
}

#[test]
fn hit_testing_does_not_materialize_rasters() {
    let view = view();
    let card = view.screen_card(0);
    view.hit(((card.x + 20) as f64, (card.y + 20) as f64));
    assert_eq!(view.composition.borrow().cache_stats().entries, 0);
    assert_eq!(view.composition.borrow().paint_stats().preparation_tasks, 0);
}

#[test]
fn hover_queries_defer_paint_only_updates() {
    let mut view = view();
    let before = view.resolved_composition().tree.frame().generation();
    for index in 0..3 {
        let card = view.screen_card(index);
        view.pointer_move(((card.x + card.width / 2) as f64, (card.y + 30) as f64));
        view.cursor();
        assert_eq!(view.hovered_card(), Some(index));
        assert_eq!(view.composition.borrow().tree.frame().generation(), before);
    }
    view.prepare_paint(Default::default());
    assert_ne!(view.composition.borrow().tree.frame().generation(), before);
}

#[test]
fn scrollbar_drag_keeps_hover_damage_but_defers_scene_updates_until_paint() {
    let mut view = view();
    let thumb = view.scrollbar();
    let point = ((thumb.x + thumb.width / 2) as f64, (thumb.y + 2) as f64);
    view.pointer_move(point);
    view.pointer_button(point, true);
    let geometry = |view: &Foundry| {
        view.composition
            .borrow()
            .geometry()
            .iter()
            .filter(|node| node.key.namespace == "foundry.card")
            .map(|node| (node.key, node.layout))
            .collect::<Vec<_>>()
    };
    let before = geometry(&view);
    let card = view.screen_card(0);
    let damage = view.pointer_move(((card.x + 10) as f64, point.1));
    assert_eq!(view.offset, 0, "top of scroll range");
    assert_eq!(
        view.hovered_card(),
        Some(0),
        "hover remains enabled while dragging"
    );
    assert!(
        damage
            .regions(view.size)
            .rects()
            .iter()
            .any(|rect| rect.contains((card.x + 10) as f64, point.1))
    );
    for event in 1..100 {
        view.pointer_move(((card.x + 10) as f64, point.1 + event as f64));
        assert_eq!(
            view.cursor(),
            retinue_ui::desktop::winit::window::CursorIcon::Default
        );
    }
    assert_eq!(geometry(&view), before);
    drop(view.resolved_composition());
    assert_ne!(geometry(&view), before);
}

#[test]
fn bottom_padding_is_in_scroll_extent_and_never_covers_cards() {
    let mut view = view();
    for scale in [1.0, 1.25, 2.0] {
        view.resize((1000, 500), scale);
        view.jump(0.0);
        assert_eq!(view.card_rect(0).y, view.viewport.y);
        view.jump(f32::MAX);
        let last = view.count() - 1;
        let rect = view.card_rect(last);
        assert_eq!(
            rect.y + rect.height - view.offset + view.px(12.0),
            view.viewport.y + view.viewport.height,
        );
        assert!(view.visible(0).contains(&last));
        let scene = view.resolved_composition();
        let bottom = scene
            .geometry()
            .iter()
            .filter(|node| node.key.namespace == "foundry.card")
            .map(|node| node.layout.y + node.layout.height)
            .reduce(f32::max)
            .unwrap();
        assert!(
            (bottom * scale - (view.viewport.y + view.viewport.height - view.px(12.0)) as f32)
                .abs()
                < 1.0
        );
    }
}

#[test]
fn retained_hit_testing_preserves_toolbar_and_scroll_priority() {
    let mut view = view();
    for scale in [1.0, 1.25, 2.0] {
        view.resize((1300, 900), scale);
        for offset in [0.0, 53.0, 800.0] {
            view.jump(offset);
            for (action, rect) in view.controls() {
                let point = (
                    (rect.x + rect.width / 2) as f64,
                    (rect.y + rect.height / 2) as f64,
                );
                assert_eq!(view.hit(point), Some(action), "{action:?} at {scale}");
            }
            assert_eq!(
                view.hit((
                    (view.size.0 - view.px(9.0)) as f64,
                    (view.viewport.y + view.px(8.0)) as f64,
                )),
                Some(Action::Scroll),
                "scroll geometry: {:?}, scale={scale}, routed={:?}",
                view.resolved_composition()
                    .bounds(retinue_ui::Key::new("foundry.overlay.control", 0)),
                view.resolved_composition().tree.hit_test((
                    (view.size.0 - view.px(9.0)) as f32 / scale,
                    (view.viewport.y + view.px(8.0)) as f32 / scale
                )),
            );
            let scene = view.resolved_composition();
            for (action, rect) in scene.controls(true) {
                assert_eq!(scene.control_bounds(action), Some(rect));
                if matches!(action, Action::Favorite(_)) {
                    let point = (
                        (rect.x + rect.width / 2) as f64,
                        (rect.y + rect.height / 2) as f64,
                    );
                    assert_eq!(view.hit(point), Some(action));
                }
            }
        }
    }
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
        .geometry()
        .iter()
        .filter(|node| node.key.namespace == "foundry.card")
        .map(|node| node.key)
        .collect::<Vec<_>>();
    assert_eq!(before.into_iter().rev().collect::<Vec<_>>(), after);
    assert_eq!(view.resolved_composition().view_stats().mounted, 0);
    assert_eq!(view.resolved_composition().tree.scene().stats.recordings, 0);
}

#[test]
fn scrolling_within_visible_rows_reuses_layout_and_recording() {
    let mut view = view();
    view.offset = 51;
    let keys = view
        .resolved_composition()
        .geometry()
        .iter()
        .map(|node| node.key)
        .collect::<Vec<_>>();
    view.offset = 52;
    let composition = view.resolved_composition();
    assert_eq!(composition.tree.scene().stats.layout_computations, 0);
    assert_eq!(composition.tree.scene().stats.recordings, 0);
    assert_eq!(composition.view_stats().mounted, 0);
    assert_eq!(composition.view_stats().bodies, 1);
    assert_eq!(
        composition
            .geometry()
            .iter()
            .map(|node| node.key)
            .collect::<Vec<_>>(),
        keys
    );
}

#[test]
fn crossing_virtual_rows_matches_a_fresh_projection() {
    for scale in [1.0, 1.25, 2.0] {
        let mut view = view();
        view.resize((1000, 760), scale);
        let descriptor = Descriptor {
            id: "test.retained-artwork".into(),
            path: "unused.png".into(),
            digest: "07".repeat(32),
        };
        for item in &mut view.snapshot.items {
            item.asset = Some(Asset {
                id: descriptor.id.clone(),
                ..Default::default()
            });
        }
        let size = view.px(105.0);
        let key = descriptor.key(size).unwrap();
        view.descriptors.insert(descriptor.id.clone(), descriptor);
        drop(view.resolved_composition());
        let stride = view.grid.height + view.grid.gap;
        for row in [1, 2, 1, 3, 0] {
            view.offset = (row * stride + 9).min(view.scroll.maximum() as u32);
            let index = view.visible(0).start;
            let rect = view.screen_card(index);
            view.pointer = ((rect.x + 20) as f64, (rect.y + 40) as f64);
            if row == 2 {
                let mut snapshot = view.snapshot.clone();
                let item = &mut snapshot.items[view.filtered[index]];
                item.pending = !item.pending;
                view.update(snapshot);
            }
            if row == 3 {
                let mut cache = retinue_render::resources::ResourceCache::new(1024 * 1024);
                let image = cache.insert(
                    key,
                    RasterImage::from_rgba(retinue_render::image::RgbaImage::from_pixel(
                        size,
                        size,
                        retinue_render::image::Rgba([12, 75, 90, 255]),
                    ))
                    .unwrap(),
                );
                view.images.insert(key, image);
                view.image_revision += 1;
            }
            let actual = render(&view);
            let retained = view.composition.replace(Default::default());
            let expected = render(&view);
            view.composition.replace(retained);
            assert_eq!(
                actual.iter().zip(&expected).position(|(a, b)| a != b),
                None,
                "scale={scale}, row={row}"
            );
        }
    }
}

#[test]
fn hover_changes_only_background_recordings_without_rebuilding_items() {
    let mut view = view();
    drop(view.resolved_composition());
    let r = view.screen_card(0);
    view.pointer = ((r.x + 5) as f64, (r.y + 5) as f64);
    {
        let composition = view.resolved_composition();
        assert_eq!(composition.view_stats().mounted, 0);
        assert_eq!(composition.tree.scene().stats.layout_computations, 0);
        assert_eq!(composition.tree.scene().stats.recordings, 1);
    }
    let r = view.screen_card(1);
    view.pointer = ((r.x + 5) as f64, (r.y + 5) as f64);
    let composition = view.resolved_composition();
    assert_eq!(composition.view_stats().mounted, 0);
    assert_eq!(composition.tree.scene().stats.layout_computations, 0);
    assert_eq!(composition.tree.scene().stats.recordings, 2);
}

#[test]
fn materialized_cards_match_direct_rendering_through_scroll_and_artwork_updates() {
    for scale in [0.9, 1.0, 1.2, 1.3, 1.5, 2.0] {
        let mut views = [view(), view()];
        for view in &mut views {
            view.resize((1000, 760), scale);
            let descriptor = Descriptor {
                id: "test.automatic-artwork".into(),
                path: "unused.png".into(),
                digest: "09".repeat(32),
            };
            for card in &mut view.snapshot.items {
                card.asset = Some(Asset {
                    id: descriptor.id.clone(),
                    ..Default::default()
                });
            }
            view.descriptors.insert(descriptor.id.clone(), descriptor);
        }
        views[1].benchmark_materializations("none");
        for row in [0, 1, 2, 1, 0] {
            for view in &mut views {
                let stride = view.grid.height + view.grid.gap;
                view.offset = (row * stride + 7).min(view.scroll.maximum() as u32);
                let index = view.visible(0).start;
                let rect = view.screen_card(index);
                view.pointer = ((rect.x + 20) as f64, (rect.y + 40) as f64);
                if row == 2 {
                    let size = view.px(105.0);
                    let key = view.descriptors["test.automatic-artwork"]
                        .key(size)
                        .unwrap();
                    let mut cache = retinue_render::resources::ResourceCache::new(1024 * 1024);
                    let image = cache.insert(
                        key,
                        RasterImage::from_rgba(retinue_render::image::RgbaImage::from_pixel(
                            size,
                            size,
                            retinue_render::image::Rgba([12, 75, 90, 255]),
                        ))
                        .unwrap(),
                    );
                    view.images.insert(key, image);
                    view.image_revision += 1;
                }
            }
            let cached = render(&views[0]);
            let stats = views[0].paint_stats();
            assert!(stats.blits > 0);
            let direct = render(&views[1]);
            assert_eq!(views[1].paint_stats().preparation_tasks, 0);
            assert_eq!(views[1].raster_stats().live_bytes, 0);
            for (actual, expected) in cached.chunks_exact(4).zip(direct.chunks_exact(4)) {
                assert_eq!(actual[3], expected[3], "alpha at scale={scale}, row={row}");
                assert!(
                    actual[..3]
                        .iter()
                        .zip(&expected[..3])
                        .all(|(a, b)| a.abs_diff(*b) <= 3),
                    "compositing at scale={scale}, row={row}: {actual:?} != {expected:?}"
                );
            }
            let repeated = render(&views[0]);
            assert_eq!(
                repeated.iter().zip(&cached).position(|(a, b)| a != b),
                None,
                "unchanged paint at scale={scale}, row={row}"
            );
            assert_eq!(views[0].paint_stats().preparation_tasks, 0);
        }
    }
}

#[test]
fn late_card_artwork_updates_one_paint_without_layout_or_item_recreation() {
    use retinue_render::image;
    let mut view = view();
    let row = view.filtered[0];
    view.snapshot.items[row].asset = Some(Asset {
        id: "test.hero".into(),
        source: "wfcd".into(),
        image_name: "hero.png".into(),
    });
    let descriptor = Descriptor {
        id: "test.hero".into(),
        path: "unused.png".into(),
        digest: "03".repeat(32),
    };
    let size = view.px(105.0);
    let key = descriptor.key(size).unwrap();
    view.descriptors.insert(descriptor.id.clone(), descriptor);
    drop(view.resolved_composition());
    let mut cache = retinue_render::resources::ResourceCache::new(1024 * 1024);
    let image = cache.insert(
        key,
        RasterImage::from_rgba(image::RgbaImage::from_pixel(
            size,
            size,
            image::Rgba([12, 75, 90, 255]),
        ))
        .unwrap(),
    );
    view.images.insert(key, image);
    view.image_revision += 1;
    let composition = view.resolved_composition();
    assert_eq!(composition.view_stats().mounted, 0);
    assert_eq!(composition.tree.scene().stats.layout_computations, 0);
    assert_eq!(composition.tree.scene().stats.recordings, 1);
}

#[test]
fn height_only_resize_mounts_new_rows_and_toolbar_motion_reuses_cards() {
    let mut view = view();
    view.resize((1300, 500), 1.25);
    let grid = view.grid;
    drop(view.resolved_composition());
    view.resize((1300, 900), 1.25);
    assert_eq!(view.grid, grid);
    {
        let composition = view.resolved_composition();
        assert_eq!(
            composition
                .geometry()
                .iter()
                .filter(|n| n.key.namespace == "foundry.card")
                .count(),
            view.visible(0).len()
        );
        assert!(composition.view_stats().mounted > 0);
    }
    let now = Instant::now();
    view.select_category(3, now);
    drop(view.resolved_composition());
    view.tick(now + Duration::from_millis(85));
    let composition = view.resolved_composition();
    assert_eq!(composition.view_stats().mounted, 0);
    assert_eq!(composition.tree.scene().stats.recordings, 0);
}

#[test]
fn card_part_branches_follow_snapshot_changes_without_recreating_the_card() {
    let mut view = view();
    let key = view
        .resolved_composition()
        .geometry()
        .iter()
        .find(|node| node.key.namespace == "foundry.card")
        .unwrap()
        .key;
    let identity = view.resolved_composition().tree.unique(key).unwrap();
    let row = view.filtered[0];
    view.snapshot.items[row].components.clear();
    view.layout_revision += 1;
    {
        let composition = view.resolved_composition();
        assert!(composition.bounds(key).is_some());
        assert_eq!(composition.view_stats().mounted, 0);
        assert!(
            composition
                .bounds(retinue_ui::Key::new("foundry.component", key.value * 8))
                .is_none()
        );
    }
    view.snapshot.items[row].components = vec![Default::default(); 8];
    view.layout_revision += 1;
    let composition = view.resolved_composition();
    assert_eq!(composition.tree.unique(key).unwrap(), identity);
    for part in 0..8 {
        assert_eq!(
            composition
                .bounds(retinue_ui::Key::new(
                    "foundry.component",
                    key.value * 8 + part
                ))
                .is_some(),
            part < 6
        );
    }
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
fn shared_component_glow_matches_vector_paint_at_all_scales() {
    use retinue_render::{
        PaintFrame, PaintInstance, PaintProgram, RasterMode, Renderer, ResidencyPolicy,
    };
    for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
        let mut view = view();
        view.resize((1300, 900), scale);
        let diameter = view.px(39.0);
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
        let command = crate::presentation::style::component_glow(
            retinue_render::Bounds::new(0.0, 0.0, diameter as f32 / scale, diameter as f32 / scale),
            scale,
        );
        let program = PaintProgram::new(
            1,
            (diameter as f32 / scale, diameter as f32 / scale),
            scale,
            Rc::new(vec![command]),
        )
        .unwrap();
        let clip = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        let instance = PaintInstance::new(1.into(), program, (r.x as i32, r.y as i32), clip);
        let mut frame = PaintFrame::default();
        frame
            .replace(size, scale, vec![instance], Vec::new())
            .unwrap();
        let mut renderer = Renderer::with_layers(
            ResidencyPolicy::desktop(0),
            retinue_render::LayerPolicy {
                min_work: 1,
                ..retinue_render::LayerPolicy::desktop()
            },
        );
        for _ in 0..3 {
            renderer.prepare(&frame, RasterMode::Replay);
            let mut warm = Painter::new(&mut pixels[0], size.0, size.1).unwrap();
            renderer.paint(&frame, &mut warm, clip, retinue_render::Color::rgb(0));
            warm.finish().unwrap();
        }
        renderer.prepare(&frame, RasterMode::Replay);
        assert_eq!(renderer.stats.prepared_fragments, 1);
        for (i, buffer) in pixels.iter_mut().enumerate() {
            let mut p = Painter::new(buffer, size.0, size.1).unwrap();
            p.fill_rounded_rect(0, 0, size.0, size.1, 0, CARD);
            p.fill_rounded_rect(r.x, r.y, r.width, r.height, diameter / 2, COMPONENT_READY);
            if i == 0 {
                renderer.draw_with(&frame, &mut p, clip, |_, _| unreachable!());
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
fn anchored_component_clusters_preserve_rectangles_hits_and_paint_only_updates() {
    let mut view = view();
    for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
        view.resize((1800, 1100), scale);
        for offset in [0, 51] {
            view.offset = offset;
            let composition = view.resolved_composition();
            let cards = composition
                .geometry()
                .into_iter()
                .filter(|node| node.key.namespace == "foundry.card");
            for (position, card_node) in view.visible(0).zip(cards) {
                let index = view.filtered[position];
                let card = &view.snapshot.items[index];
                let screen = view.card_rect(position);
                for (part, rect) in view.parts(card.components.len()).into_iter().enumerate() {
                    let component = &card.components[part];
                    let id = card_node.key.value * 8 + part as u64;
                    let node = composition
                        .bounds(retinue_ui::Key::new("foundry.component", id))
                        .unwrap();
                    assert_eq!(node.parent.unwrap().namespace, "foundry.component.cluster");
                    assert_eq!((node.layout.x * scale).round() as u32, screen.x + rect.x);
                    assert_eq!(
                        (node.layout.y * scale).round() as i64,
                        (screen.y + rect.y) as i64 - view.offset as i64
                    );
                    assert_eq!((node.layout.width * scale).round() as u32, rect.width);
                    assert_eq!((node.layout.height * scale).round() as u32, rect.height);
                    for (namespace, expected, leading, dx, dy) in [
                        (
                            "foundry.component.count",
                            view.count_badge(
                                rect,
                                &component.owned.to_string(),
                                component.owned_relic,
                            ),
                            false,
                            3.0,
                            2.0,
                        ),
                        (
                            "foundry.component.relic",
                            view.relic_badge(rect),
                            true,
                            -2.0,
                            3.0,
                        ),
                    ] {
                        let badge = composition
                            .bounds(retinue_ui::Key::new(namespace, id))
                            .unwrap()
                            .layout;
                        let edge = if leading {
                            badge.x - node.layout.x
                        } else {
                            badge.x + badge.width - node.layout.x - node.layout.width
                        };
                        assert!((edge - dx).abs() < 0.001, "{namespace} scale={scale}");
                        assert!(
                            (badge.y + badge.height - node.layout.y - node.layout.height - dy)
                                .abs()
                                < 0.001
                        );
                        assert_eq!((badge.width * scale).round() as u32, expected.width);
                        assert_eq!((badge.height * scale).round() as u32, expected.height);
                    }
                    let point = (
                        ((node.layout.x + node.layout.width / 2.0) * scale) as f64,
                        ((node.layout.y + node.layout.height / 2.0) * scale) as f64,
                    );
                    if node.hit.contains(point.0, point.1) {
                        assert_eq!(composition.hit(point), Some(Action::Component(index, part)));
                    }
                }
            }
            drop(composition);
            view.image_revision += 1;
            assert_eq!(
                view.resolved_composition()
                    .tree
                    .scene()
                    .stats
                    .layout_computations,
                0
            );
        }
    }
}

#[test]
fn card_palette_and_hover_damage_match_full_repaint() {
    use retinue_render::painter::render_thread_budget;
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
    let mut cache = retinue_render::resources::ResourceCache::new(1024);
    let image = cache.insert(
        key,
        RasterImage::from_rgba(retinue_render::image::RgbaImage::new(2, 2)).unwrap(),
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
    let before = view.resolved_composition().geometry().len();
    render(&view);
    assert!(view.composition.borrow().cache_stats().live_bytes > 0);
    view.clear_images();
    assert_eq!(view.composition.borrow().cache_stats().live_bytes, 0);
    assert_eq!(view.composition.borrow().cache_stats().warm_bytes, 0);
    assert_eq!(view.resolved_composition().geometry().len(), before);
    assert_eq!(view.composition.borrow().cache_stats().live_bytes, 0);
    render(&view);
    assert!(view.composition.borrow().cache_stats().live_bytes > 0);
}

#[test]
fn hidden_foundry_releases_draw_resources_but_keeps_model_and_scroll() {
    let mut view = view();
    let before = view.count();
    render(&view);
    assert!(view.composition.borrow().cache_stats().live_bytes > 0);
    view.set_active(false);
    assert_eq!(view.composition.borrow().cache_stats().live_bytes, 0);
    assert!(
        view.image_requests().len() <= 1,
        "only the player emblem stays visible"
    );
    assert!(view.desired_assets().len() <= 1);
    assert_eq!(view.count(), before);
    for scale in [1.0, 1.25, 1.5, 2.0, 1.0] {
        view.resize((900, 900), scale);
        view.controls();
        view.item_controls();
        assert_eq!(
            view.composition.borrow().cache_stats().live_bytes,
            0,
            "hidden layout queries must not recreate card rasters"
        );
    }
    view.set_active(true);
    drop(view.resolved_composition());
    assert_eq!(view.composition.borrow().cache_stats().live_bytes, 0);
    render(&view);
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
    use retinue_render::painter::render_thread_budget;
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
    let args = crate::app::Args::parse_from(["wfgui", "--size", "2560x1440", "--ui-scale", "125"]);
    let mut shell = crate::presentation::shell::Shell::new(
        Some(crate::data::storage::Snapshot::fixture()),
        1.25,
    )
    .unwrap();
    shell.resize(args.size, 1.0).unwrap();
    shell.set_page(crate::presentation::shell::Page::Foundry);
    crate::app::prepare_capture(&mut shell, &args).unwrap();
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
    use retinue_render::{damage::AgeRepaintTracker, painter::render_thread_budget};
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
        painter.repair(&plan.repair);
        let repair = plan.repair.bounds();
        painter.fill_rounded_rect(repair.x, repair.y, repair.width, repair.height, 0, PANEL);
        view.paint(&mut painter, Default::default());
        painter.finish().unwrap();
        let mut expected = vec![0u8; buffer.len()];
        let mut painter =
            Painter::with_threads(&mut expected, size.0, size.1, render_thread_budget(4)).unwrap();
        painter.fill_rounded_rect(0, 0, size.0, size.1, 0, PANEL);
        {
            // Compare repairs against the same execution plan, not a new cache
            // decision caused by requesting a full repaint for the oracle.
            let composition = view.composition.borrow();
            composition.renderer.borrow_mut().draw_with(
                composition.tree.frame(),
                &mut painter,
                Rect {
                    width: size.0,
                    height: size.1,
                    ..Rect::default()
                },
                |_, p| {
                    let r = view.search_bounds();
                    p.translated((-(r.x as i32), -(r.y as i32)), |p| view.paint_search(p, r));
                },
            );
        }
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
    use retinue_render::{image, resources::ResourceCache};
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
    use rustix::{time::ClockId, time::clock_gettime};
    let cpu_ns = |clock| {
        let time = clock_gettime(clock);
        time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
    };
    let threads = crate::app::benchmark::threads();
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
        let args = crate::app::Args::parse_from(args);
        let mut shell = crate::presentation::shell::Shell::new(
            Some(crate::data::storage::Snapshot::fixture()),
            scale as f32 / 100.0,
        )
        .unwrap();
        shell.resize((w, h), 1.0).unwrap();
        shell.set_page(crate::presentation::shell::Page::Foundry);
        crate::app::prepare_capture(&mut shell, &args).unwrap();
        let mut buffer = vec![0u32; w as usize * h as usize];
        for full in [false, true] {
            let mut samples = Vec::new();
            let mut phases = Vec::new();
            #[cfg(feature = "profiling")]
            let mut sdk_phases = Vec::new();
            let mut actual_threads = (u32::MAX, 0);
            let mut last = None;
            for frame in 0..180 {
                let process = cpu_ns(ClockId::ProcessCPUTime);
                let caller = cpu_ns(ClockId::ThreadCPUTime);
                let start = Instant::now();
                let offset = ((frame % 120).min(120 - frame % 120)) as f32 * 2.0;
                shell.foundry.jump(offset);
                let update_stats = {
                    let composition = shell.foundry.resolved_composition();
                    #[cfg(feature = "profiling")]
                    if frame >= 20 {
                        let profile = composition.tree.profile;
                        let scene = composition.tree.scene().profile;
                        sdk_phases.push(serde_json::json!({
                            "evaluation_ms": profile.evaluation.as_secs_f64() * 1000.0,
                            "lowering_ms": profile.lowering.as_secs_f64() * 1000.0,
                            "scene_ms": profile.scene.as_secs_f64() * 1000.0,
                            "input_ms": profile.input.as_secs_f64() * 1000.0,
                            "bodies": composition.tree.stats.bodies,
                            "projected": composition.tree.stats.projected,
                            "mounted": composition.tree.stats.mounted,
                            "scene_parts_ms": ([scene.reconcile, scene.layout, scene.projection,
                                scene.geometry, scene.frame].map(|d| d.as_secs_f64() * 1000.0)),
                        }));
                    }
                    composition.tree.scene().stats
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
                    crate::presentation::shell::BACKGROUND,
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
                    let prepared = composition.paint_stats();
                    #[cfg(feature = "profiling")]
                    {
                        let sample = sdk_phases.last_mut().unwrap();
                        let [chrome, prepare, draw] = composition.paint_profile.get();
                        sample["chrome_update_ms"] =
                            serde_json::json!(chrome.as_secs_f64() * 1000.0);
                        sample["prepare_ms"] = serde_json::json!(prepare.as_secs_f64() * 1000.0);
                        sample["draw_ms"] = serde_json::json!(draw.as_secs_f64() * 1000.0);
                        sample["inspected_instances"] =
                            serde_json::json!(prepared.inspected_instances);
                        sample["changed_programs"] = serde_json::json!(prepared.changed_programs);
                        sample["shell_paint_ms"] = serde_json::json!(
                            shell.paint_profile.get().map(|d| d.as_secs_f64() * 1000.0)
                        );
                        sample["record_ms"] = serde_json::json!(
                            composition.record_profile.map(|d| d.as_secs_f64() * 1000.0)
                        );
                    }
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
                            prepared.blits,
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
            reports.push(serde_json::json!({"size":[w,h],"scale":scale,"threads":threads.get(),"actual_threads":actual_threads,"live":live,"mode":if full{"full_window"}else{"viewport_repaint"},"columns":shell.foundry.grid.columns,"visible_cards":shell.foundry.visible(0).len(),"total_cards":shell.foundry.snapshot.items.len(),"prepared_images":shell.foundry.prepared_count(),"decoded_source_bytes":stats.0.live_bytes,"thumbnail_bytes":stats.1.live_bytes,"image_errors":shell.foundry.image_errors,"p50_ms":sorted[sorted.len()/2],"p95_ms":sorted[(sorted.len()-1)*95/100],"max_ms":sorted.last(),"samples_ms":samples,"phases":phases}));
            #[cfg(feature = "profiling")]
            {
                reports.last_mut().unwrap()["sdk_phases"] = serde_json::json!(sdk_phases);
            }
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
