use super::*;
use crate::{damage::AgeRepaintTracker, painter::Painter};
use std::time::{Duration, Instant};
use taffy::{Dimension, Point};

fn fixed(width: f32, height: f32) -> Style {
    Style {
        size: Size {
            width: Dimension::length(width),
            height: Dimension::length(height),
        },
        flex_shrink: 0.0,
        ..Style::default()
    }
}

fn fill(key: u64, color: [u8; 4]) -> Paint {
    Paint::new(key, move |size| {
        vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, size.width, size.height),
            radius: 5.0,
            color: Color::rgba(color[0], color[1], color[2], color[3]),
        }]
    })
}

fn child(key: u64) -> Element {
    let mut element = Element::new(key, fixed(62.0, 42.0));
    element.paint = Some(fill(key, [30 + (key % 60) as u8 * 3, 70, 130, 255]));
    element.interactive = true;
    element
}

#[test]
fn scoped_keys_separate_instances_and_cached_content() {
    let children = ["foundry", "relics"]
        .into_iter()
        .enumerate()
        .map(|(index, scope)| {
            let key = Key::new(scope, 1);
            let mut item = Element::new(key, fixed(20.0, 20.0));
            item.paint = Some(Paint::new(PaintKey::new(key, 0), move |_| {
                vec![Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 20.0, 20.0),
                    radius: 0.0,
                    color: Color::rgb(if index == 0 { 0xff0000 } else { 0x00ff00 }),
                }]
            }));
            item.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
            item
        })
        .collect();
    let mut scene = Scene::default();
    scene
        .update(
            Element::row(0, 0.0, children),
            (40, 20),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    let mut renderer = Renderer::new(16 * 1024);
    renderer.prepare(&scene, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 2);
    let mut pixels = vec![0; 40 * 20 * 4];
    render(
        &scene,
        &mut renderer,
        &mut pixels,
        (40, 20),
        Damage::Full.bounds((40, 20)).unwrap(),
    );
    assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
    assert_eq!(&pixels[20 * 4..21 * 4], &[0, 255, 0, 255]);
}

#[test]
fn custom_paint_is_live_clipped_and_invalidates_local_physical_pixels() {
    for scale in [1.0, 1.25, 2.0] {
        let key = Key::new("page", 1);
        let mut item = Element::new(key, fixed(20.0, 20.0));
        item.offset = (8.0, 4.0);
        item.paint = Some(Paint::external(key));
        item.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
        let mut scene = Scene::default();
        scene
            .update(item, (100, 100), scale, RecordingMode::Retained)
            .unwrap();
        let bounds = scene.bounds(key).unwrap().paint;
        assert_eq!(scene.invalidate(key, Damage::Full), Damage::rect(bounds));
        assert_eq!(
            scene.invalidate(
                key,
                Damage::rect(Rect {
                    x: 2,
                    y: 3,
                    width: 4,
                    height: 5
                })
            ),
            Damage::rect(Rect {
                x: bounds.x + 2,
                y: bounds.y + 3,
                width: 4,
                height: 5
            })
        );
        let mut renderer = Renderer::new(128 * 1024);
        renderer.prepare(&scene, RasterMode::Explicit);
        assert_eq!(renderer.stats.prepared_rasters, 0);
        let mut pixels = vec![0; 100 * 100 * 4];
        for color in [Color::rgb(0xff0000), Color::rgb(0x00ff00)] {
            let mut painter = Painter::new(&mut pixels, 100, 100).unwrap();
            renderer.paint_with(
                &scene,
                &mut painter,
                Rect {
                    width: 100,
                    height: 100,
                    ..Rect::default()
                },
                Color::rgb(0),
                |id, p| {
                    assert_eq!(id, key);
                    p.fill_rounded_rect(0, 0, 100, 100, 0, color.bgra());
                },
            );
            painter.finish().unwrap();
            let pixel = ((bounds.y * 100 + bounds.x) * 4) as usize;
            assert_eq!(&pixels[pixel..pixel + 4], &color.bgra());
            let outside = ((bounds.y * 100 + bounds.x + bounds.width) * 4) as usize;
            assert_eq!(&pixels[outside..outside + 4], &[0, 0, 0, 255]);
        }
    }
}

#[test]
fn adding_a_disjoint_leaf_does_not_invalidate_later_unchanged_layers() {
    let root = |added| {
        let mut a = child(1);
        a.style.position = taffy::Position::Absolute;
        let mut b = child(2);
        b.style.position = taffy::Position::Absolute;
        b.offset = (100.0, 0.0);
        let mut children = vec![a];
        if added {
            let mut item = child(3);
            item.style.position = taffy::Position::Absolute;
            item.offset = (0.0, 60.0);
            children.push(item);
        }
        children.push(b);
        let mut root = Element::new(0, fixed(200.0, 140.0));
        root.children = children;
        root
    };
    let mut scene = Scene::default();
    scene
        .update(root(false), (200, 140), 1.0, RecordingMode::Retained)
        .unwrap();
    let damage = scene
        .update(root(true), (200, 140), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(
        damage,
        Damage::rect(Rect {
            x: 0,
            y: 60,
            width: 62,
            height: 42
        })
    );
    let damage = scene
        .update(root(false), (200, 140), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(
        damage,
        Damage::rect(Rect {
            x: 0,
            y: 60,
            width: 62,
            height: 42
        })
    );
}

fn scene(frame: usize) -> Element {
    let mut items: Vec<_> = (1..8)
        .filter(|id| !(frame >= 4 && *id == 3))
        .map(child)
        .collect();
    if frame >= 5 {
        items.reverse();
    }
    let changed = items.iter_mut().find(|item| item.key == 2.into()).unwrap();
    if frame >= 2 {
        changed.paint = Some(fill(100 + frame as u64, [170, 20, 50, 255]));
    }
    for item in &mut items {
        item.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
    }
    let mut content = Element::column(10, 9.0, items);
    content.offset = (-3.5, -(frame as f32) * 9.0);
    content.style.flex_shrink = 0.0;
    let mut root = Element::column(11, 0.0, vec![content]);
    root.style = fixed(120.0, 160.0);
    root.clip = true;
    root
}

fn render(
    scene: &Scene,
    renderer: &mut Renderer,
    pixels: &mut [u8],
    size: (u32, u32),
    repair: Rect,
) {
    let mut painter = Painter::new(pixels, size.0, size.1).unwrap();
    renderer.paint(scene, &mut painter, repair, Color::rgb(0x121419));
    painter.finish().unwrap();
}

#[test]
fn rotating_buffers_match_full_paint_after_scroll_reorder_removal_and_updates() {
    for scale in [1.0, 1.25, 2.0] {
        let size = ((140.0 * scale) as u32, (170.0 * scale) as u32);
        let length = size.0 as usize * size.1 as usize * 4;
        let mut buffers = [vec![0; length], vec![0; length], vec![0; length]];
        let mut oracle = vec![0; length];
        let mut retained = Scene::default();
        let mut full = Scene::default();
        let mut renderer = Renderer::new(64 * 1024);
        let mut direct = Renderer::new(0);
        let mut tracker = AgeRepaintTracker::default();
        for frame in 0..12 {
            let damage = retained
                .update(scene(frame), size, scale, RecordingMode::Retained)
                .unwrap();
            full.update(scene(frame), size, scale, RecordingMode::Always)
                .unwrap();
            tracker.invalidate(damage);
            let repair = tracker.plan(if frame < 3 { 0 } else { 3 }, size).unwrap();
            renderer.prepare(&retained, RasterMode::Replay);
            direct.prepare(&full, RasterMode::Replay);
            render(
                &retained,
                &mut renderer,
                &mut buffers[frame % 3],
                size,
                repair.repair,
            );
            render(
                &full,
                &mut direct,
                &mut oracle,
                size,
                Damage::Full.bounds(size).unwrap(),
            );
            assert_eq!(buffers[frame % 3], oracle, "scale={scale}, frame={frame}");
            tracker.submitted();
        }
    }
}

#[test]
fn raster_cache_eviction_and_fractional_scale_keep_pixels_equivalent() {
    for scale in [1.0, 1.25, 2.0] {
        let size = ((140.0 * scale) as u32, (170.0 * scale) as u32);
        let mut retained = Scene::default();
        let mut cached = Renderer::new(16 * 1024);
        let mut direct = Renderer::new(0);
        for frame in 0..12 {
            retained
                .update(scene(frame), size, scale, RecordingMode::Retained)
                .unwrap();
            if frame == 5 {
                cached.clear_cache();
            }
            cached.prepare(&retained, RasterMode::Explicit);
            direct.prepare(&retained, RasterMode::Replay);
            let mut a = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut b = a.clone();
            let repair = Damage::Full.bounds(size).unwrap();
            render(&retained, &mut cached, &mut a, size, repair);
            render(&retained, &mut direct, &mut b, size, repair);
            let max = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            assert!(
                max <= 1,
                "source-over rounding: scale={scale}, frame={frame}, max={max}"
            );
        }
    }
}

#[test]
fn scrolling_cached_items_repairs_only_changed_regions_and_survives_reorder() {
    for scale in [1.0, 1.25, 2.0] {
        let size = ((180.0 * scale) as u32, (150.0 * scale) as u32);
        let mut scene = Scene::default();
        let mut cached = Renderer::new(2 * 1024 * 1024);
        let mut direct = Renderer::new(0);
        let mut tracker = AgeRepaintTracker::default();
        let mut buffers = vec![vec![0; size.0 as usize * size.1 as usize * 4]; 3];
        for frame in 0..14 {
            let mut root = Element::new(0, fixed(180.0, 150.0));
            root.clip = true;
            for id in 1..=4 {
                let position = if frame >= 7 { 5 - id } else { id };
                let mut item = Element::at(
                    id,
                    Bounds::new(8.0, (position - 1) as f32 * 68.0, 155.0, 60.0),
                );
                item.offset.1 = -((frame * 11) as f32);
                let revision = if id == 2 && frame >= 3 { 1 } else { 0 };
                item.paint = Some(Paint::new(PaintKey::new(id.into(), revision), move |_| {
                    vec![
                        Command::Fill {
                            bounds: Bounds::new(0.0, 0.0, 155.0, 60.0),
                            radius: 7.0,
                            color: Color::rgba(30, 60, 90, 140),
                        },
                        Command::Fill {
                            bounds: Bounds::new(12.0, 10.0, 18.0, 16.0),
                            radius: 5.0,
                            color: Color::rgba(if revision == 0 { 220 } else { 50 }, 60, 90, 180),
                        },
                        Command::Fill {
                            bounds: Bounds::new(20.0, 18.0, 18.0, 16.0),
                            radius: 5.0,
                            color: Color::rgba(40, 190, 90, 180),
                        },
                    ]
                }));
                item.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
                root.children.push(item);
            }
            let damage = scene
                .update(root, size, scale, RecordingMode::Retained)
                .unwrap();
            if frame == 9 {
                cached.clear_cache();
            }
            cached.prepare(&scene, RasterMode::Explicit);
            if frame == 1 {
                assert_eq!(cached.stats.prepared_rasters, 0, "scroll must reuse items");
            }
            if frame == 3 {
                assert!(cached.stats.copied_pixels > 0);
                assert!(cached.stats.repaired_pixels < cached.stats.copied_pixels / 4);
            }
            direct.prepare(&scene, RasterMode::Replay);
            tracker.invalidate(damage);
            let repair = tracker.plan(if frame < 3 { 0 } else { 3 }, size).unwrap();
            render(
                &scene,
                &mut cached,
                &mut buffers[frame % 3],
                size,
                repair.repair,
            );
            let mut oracle = vec![0; buffers[0].len()];
            render(
                &scene,
                &mut direct,
                &mut oracle,
                size,
                Damage::Full.bounds(size).unwrap(),
            );
            let max = buffers[frame % 3]
                .iter()
                .zip(&oracle)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(max <= 2, "frame={frame}, scale={scale}, max={max}");
            assert!(cached.cache_stats().cached_bytes <= cached.cache_stats().budget_bytes);
            tracker.submitted();
        }
    }
}

#[test]
fn nested_live_paint_is_not_raster_cached() {
    let key = Key::new("live", 1);
    let mut item = Element::new(key, fixed(20.0, 20.0));
    item.paint = Some(Paint::new(key.value, move |_| {
        vec![Command::Clip {
            bounds: Bounds::new(0.0, 0.0, 20.0, 20.0),
            commands: Rc::new(vec![Command::External {
                bounds: Bounds::new(0.0, 0.0, 20.0, 20.0),
                key,
            }]),
        }]
    }));
    item.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
    let mut scene = Scene::default();
    scene
        .update(item, (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    let mut renderer = Renderer::new(4096);
    renderer.prepare(&scene, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 0);
}

#[test]
fn command_canvas_matches_native_nested_clips_and_resource_images() {
    use crate::{
        image,
        painter::{GlyphCache, RasterImage, load_overlay_font},
        resources::ResourceCache,
    };
    for scale in [1.0, 1.25, 2.0] {
        let mut resources = ResourceCache::new(8192);
        let image = resources.insert(
            0u64,
            RasterImage::from_rgba(image::RgbaImage::from_fn(13, 9, |x, y| {
                image::Rgba([(x * 17) as u8, (y * 23) as u8, 110, 190])
            }))
            .unwrap(),
        );
        let font = Rc::new(load_overlay_font(&GlyphCache::new(8192)).unwrap());
        let rect = Rect {
            x: 5,
            y: 7,
            width: 64,
            height: 54,
        };
        let picture = Rect {
            x: 1,
            y: 2,
            width: 43,
            height: 32,
        };
        let inner = Rect {
            x: 8,
            y: 11,
            width: 40,
            height: 25,
        };
        let mut canvas = Canvas::new(scale);
        canvas.viewport(rect, (-3, 4), |c| {
            c.image(picture, image.clone(), ImageFit::Circle);
            c.viewport(inner, (7, -2), |c| {
                c.image(picture, image.clone(), ImageFit::Cover)
            });
            c.text(
                Rect {
                    x: 0,
                    y: 12,
                    width: 80,
                    height: 25,
                },
                font.clone(),
                "Relic".into(),
                13.0,
                [240, 230, 220, 255],
            );
        });
        let mut element = Element::new(1, fixed(120.0, 90.0));
        element.offset = (3.25, 9.1);
        element.paint = Some(Paint::new(1, move |_| canvas.commands.clone()));
        let size = (320, 240);
        let mut scene = Scene::default();
        scene
            .update(element, size, scale, RecordingMode::Retained)
            .unwrap();
        let mut renderer = Renderer::new(0);
        renderer.prepare(&scene, RasterMode::Replay);
        let mut actual = vec![0; 320 * 240 * 4];
        let mut expected = actual.clone();
        let frame = Damage::Full.bounds(size).unwrap();
        let mut p = Painter::new(&mut actual, 320, 240).unwrap();
        renderer.draw_with(&scene, &mut p, frame, |_, _| unreachable!());
        p.finish().unwrap();
        let mut p = Painter::new(&mut expected, 320, 240).unwrap();
        p.viewport(
            frame,
            ((3.25 * scale).round() as i32, (9.1 * scale).round() as i32),
            |p| {
                p.viewport(rect, (-3, 4), |p| {
                    p.draw_image_contained_circle(&image, picture.x, picture.y, picture.width);
                    p.viewport(inner, (7, -2), |p| p.draw_image_covered(&image, picture));
                    p.viewport(
                        Rect {
                            x: 0,
                            y: 12,
                            width: 80,
                            height: 25,
                        },
                        (0, 0),
                        |p| {
                            p.draw_text_vertically_centered(
                                font.as_ref(),
                                crate::painter::TextLine::new(0, 12, 25),
                                13.0 * scale,
                                "Relic",
                                [240, 230, 220, 255],
                            )
                        },
                    );
                });
            },
        );
        p.finish().unwrap();
        let mismatches = actual
            .iter()
            .zip(&expected)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(8)
            .map(|(i, (a, b))| (i / 4 % 320, i / 4 / 320, i % 4, *a, *b))
            .collect::<Vec<_>>();
        assert!(
            mismatches.is_empty(),
            "scale={scale}, first differing channels={mismatches:?}"
        );
    }
}

#[test]
fn command_transactions_reject_invalid_or_duplicate_keys_without_partial_updates() {
    let mut scene = Scene::default();
    scene
        .update(
            Element::row(0, 0.0, vec![child(1), child(2)]),
            (200, 100),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    let before = scene.bounds(1.into()).unwrap().paint;
    assert!(
        scene
            .record_many([
                (1.into(), Vec::new(), CachePolicy::Never),
                (3.into(), Vec::new(), CachePolicy::Never)
            ])
            .is_err()
    );
    assert_eq!(scene.bounds(1.into()).unwrap().paint, before);
    assert!(
        scene
            .record_many([
                (1.into(), Vec::new(), CachePolicy::Never),
                (1.into(), Vec::new(), CachePolicy::Never)
            ])
            .is_err()
    );
    assert_eq!(scene.bounds(1.into()).unwrap().paint, before);
}

#[test]
fn unchanged_tree_reuses_layout_and_recording_and_requests_no_damage() {
    let mut retained = Scene::default();
    retained
        .update(scene(0), (140, 170), 1.0, RecordingMode::Retained)
        .unwrap();
    let damage = retained
        .update(scene(0), (140, 170), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(damage, Damage::None);
    assert_eq!(retained.stats.layout_computations, 0);
    assert_eq!(retained.stats.recordings, 0);
    retained
        .update(scene(1), (140, 170), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(retained.stats.layout_computations, 0);
    assert_eq!(retained.stats.recordings, 0);
}

#[test]
fn resizing_constraints_preserves_fixed_children_and_recordings() {
    let mut root = Element::row(0, 0.0, vec![child(1)]);
    root.style.size = Size {
        width: Dimension::percent(1.0),
        height: Dimension::percent(1.0),
    };
    let mut scene = Scene::default();
    scene
        .update(root, (200, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(scene.resize((300, 150), 1.0), Ok(Damage::Full));
    assert_eq!(scene.bounds(0.into()).unwrap().hit.width, 300);
    assert_eq!(scene.bounds(1.into()).unwrap().hit.width, 62);
    assert_eq!(scene.stats.recordings, 0);
    assert!(scene.resize((50, 50), 0.0).is_err());
    assert_eq!(scene.bounds(0.into()).unwrap().hit.width, 300);
    assert_eq!(scene.resize((300, 150), 1.0), Ok(Damage::None));
    assert_eq!(scene.stats.layout_computations, 0);
}

#[test]
fn layout_changes_move_siblings_but_paint_changes_do_not_relayout() {
    let describe = |width, color| {
        let mut first = child(1);
        first.style.size.width = Dimension::length(width);
        first.paint = Some(fill(color, [color as u8, 70, 130, 255]));
        Element::row(0, 2.0, vec![first, child(2)])
    };
    let mut scene = Scene::default();
    scene
        .update(describe(62.0, 20), (170, 60), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(scene.bounds(2.into()).unwrap().layout.x, 64.0);
    scene
        .update(describe(96.0, 20), (170, 60), 1.0, RecordingMode::Retained)
        .unwrap();
    assert!(scene.stats.layout_computations > 0);
    assert_eq!(scene.stats.recordings, 1);
    assert_eq!(scene.bounds(2.into()).unwrap().layout.x, 98.0);
    let before = scene.bounds(2.into()).unwrap().layout;
    scene
        .update(describe(96.0, 30), (170, 60), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(scene.stats.layout_computations, 0);
    assert_eq!(scene.stats.recordings, 1);
    assert_eq!(scene.bounds(2.into()).unwrap().layout, before);
}

#[test]
fn scoped_updates_skip_clean_geometry_and_reject_cross_scope_keys() {
    let describe = |color| {
        let mut first = child(1);
        first.paint = Some(fill(color, [color as u8, 70, 130, 255]));
        Element::row(
            0,
            2.0,
            vec![Element::column(10, 2.0, vec![first, child(2)]), child(3)],
        )
    };
    let mut scene = Scene::default();
    scene
        .update(describe(20), (170, 100), 1.25, RecordingMode::Retained)
        .unwrap();
    let clean = scene.bounds(3.into()).unwrap().layout;
    let replacement = describe(30).children.remove(0).children.remove(0);
    scene.update_subtree(replacement).unwrap();
    assert_eq!(scene.stats.nodes, 1);
    assert_eq!(scene.stats.layout_computations, 0);
    assert_eq!(scene.stats.recordings, 1);
    assert_eq!(scene.bounds(3.into()).unwrap().layout, clean);
    let mut oracle = Scene::default();
    oracle
        .update(describe(30), (170, 100), 1.25, RecordingMode::Always)
        .unwrap();
    for expected in oracle.geometry() {
        let actual = scene.bounds(expected.key).unwrap();
        assert_eq!(actual.layout, expected.layout);
        assert_eq!(actual.paint, expected.paint);
    }
    let generation = scene.generation;
    assert!(
        scene
            .update_subtree(Element::column(10, 0.0, vec![child(3)]))
            .is_err()
    );
    assert!(scene.update_subtree(child(99)).is_err());
    assert_eq!(scene.generation, generation);
    assert_eq!(scene.bounds(3.into()).unwrap().layout, clean);
}

#[test]
fn scoped_layout_and_topology_changes_match_full_reconciliation() {
    let describe = |step| {
        let mut items = vec![child(1), child(2)];
        if step > 0 {
            items[0].style.size.width = Dimension::length(90.0);
        }
        if step > 1 {
            items.push(child(4));
            items.reverse();
        }
        if step > 2 {
            items[1].style.display = taffy::Display::None;
        }
        Element::row(0, 2.0, vec![Element::column(10, 2.0, items), child(3)])
    };
    for scale in [1.0, 1.25, 2.0] {
        let size = (400, 300);
        let mut retained = Scene::default();
        retained
            .update(describe(0), size, scale, RecordingMode::Retained)
            .unwrap();
        for step in [1, 2, 3, 0] {
            let root = describe(step);
            retained
                .update_subtree(root.children.into_iter().next().unwrap())
                .unwrap();
            let mut oracle = Scene::default();
            oracle
                .update(describe(step), size, scale, RecordingMode::Always)
                .unwrap();
            assert_eq!(retained.geometry().len(), oracle.geometry().len());
            for expected in oracle.geometry() {
                let actual = retained.bounds(expected.key).unwrap();
                assert_eq!(actual.layout, expected.layout, "scale {scale} step {step}");
                assert_eq!(actual.hit, expected.hit);
                assert_eq!(actual.paint, expected.paint);
            }
        }
    }
}

#[test]
fn sibling_patch_ranges_repair_ancestor_overflow_and_reappearing_content() {
    let mut root = Element::row(
        0,
        2.0,
        vec![Element::column(10, 0.0, vec![child(1)]), child(2)],
    );
    let mut scene = Scene::default();
    scene
        .update(root, (240, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    let wider = Paint::new(99, |_| {
        vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, 110.0, 45.0),
            radius: 0.0,
            color: Color::rgb(0xffffff),
        }]
    });
    scene
        .patch([
            Change::Paint(1.into(), Some(fill(98, [10, 20, 30, 255]))),
            Change::Paint(2.into(), Some(wider.clone())),
        ])
        .unwrap();
    root = Element::row(
        0,
        2.0,
        vec![Element::column(10, 0.0, vec![child(1)]), child(2)],
    );
    root.children[0].children[0].paint = Some(fill(98, [10, 20, 30, 255]));
    root.children[1].paint = Some(wider);
    let mut oracle = Scene::default();
    oracle
        .update(root, (240, 100), 1.0, RecordingMode::Always)
        .unwrap();
    assert_eq!(
        scene.bounds(0.into()).unwrap().paint,
        oracle.bounds(0.into()).unwrap().paint
    );
    let original = scene.bounds(0.into()).unwrap().paint;
    for offset in [-400.0, 400.0, 0.0] {
        scene
            .patch([Change::Offset(0.into(), (offset, 0.0))])
            .unwrap();
        assert_eq!(scene.stats.recordings, 0);
    }
    assert_eq!(scene.bounds(0.into()).unwrap().paint, original);
    assert_eq!(scene.stats.visible_chunks, 2);
}

#[test]
fn unchanged_patches_skip_projection_but_keep_sequential_writes() {
    let mut scene = Scene::default();
    let root = Element::row(0, 0.0, vec![child(1), child(2)]);
    let paint = root.children[0].paint.clone();
    scene
        .update(root, (160, 100), 1.25, RecordingMode::Retained)
        .unwrap();
    let generation = scene.generation;
    let before = scene.bounds(1.into()).unwrap().layout;
    let damage = scene
        .patch([
            Change::Offset(1.into(), (0.0, 0.0)),
            Change::Paint(1.into(), paint),
        ])
        .unwrap();
    assert_eq!(damage, Damage::None);
    assert_eq!(scene.generation, generation);
    assert_eq!(scene.stats.nodes, 0);
    assert_eq!(scene.stats.recordings, 0);
    assert_eq!(scene.stats.layout_computations, 0);
    scene
        .patch([
            Change::Offset(1.into(), (7.0, 0.0)),
            Change::Offset(1.into(), (0.0, 0.0)),
            Change::Paint(1.into(), None),
            Change::Paint(1.into(), Some(fill(123, [10, 20, 30, 255]))),
        ])
        .unwrap();
    assert_eq!(scene.bounds(1.into()).unwrap().layout, before);
    assert_eq!(scene.stats.recordings, 1);
    assert_eq!(scene.stats.layout_computations, 0);
}

#[test]
fn presentation_patches_match_relayout_with_recycled_buffers() {
    for scale in [1.0, 1.25, 2.0] {
        let size = ((140.0 * scale) as u32, (170.0 * scale) as u32);
        let mut retained = Scene::default();
        let mut oracle = Scene::default();
        let mut renderer = Renderer::new(0);
        let mut direct = Renderer::new(0);
        let length = size.0 as usize * size.1 as usize * 4;
        let mut buffers = [vec![0; length], vec![0; length], vec![0; length]];
        let mut expected = vec![0; length];
        let mut tracker = AgeRepaintTracker::default();
        retained
            .update(scene(0), size, scale, RecordingMode::Retained)
            .unwrap();
        for frame in 0..10 {
            let offset = (-3.5, -(frame as f32) * 6.25);
            let mut full = scene(0);
            full.children[0].offset = offset;
            let paint = (frame >= 5).then(|| fill(100, [180, 90, 60, 255]));
            full.children[0].children[1].paint = paint.clone();
            oracle
                .update(full, size, scale, RecordingMode::Always)
                .unwrap();
            let damage = retained
                .patch([
                    Change::Offset(10.into(), offset),
                    Change::Paint(2.into(), paint),
                ])
                .unwrap();
            assert_eq!(retained.stats.layout_computations, 0);
            assert!(retained.stats.recordings <= 1);
            for expected in oracle.geometry() {
                let actual = retained.bounds(expected.key).unwrap();
                assert_eq!(actual.layout, expected.layout);
                assert_eq!(actual.paint, expected.paint);
                assert_eq!(actual.hit, expected.hit);
            }
            tracker.invalidate(damage);
            let repair = tracker.plan(if frame < 3 { 0 } else { 3 }, size).unwrap();
            renderer.prepare(&retained, RasterMode::Replay);
            direct.prepare(&oracle, RasterMode::Replay);
            render(
                &retained,
                &mut renderer,
                &mut buffers[frame % 3],
                size,
                repair.repair,
            );
            render(
                &oracle,
                &mut direct,
                &mut expected,
                size,
                Damage::Full.bounds(size).unwrap(),
            );
            assert_eq!(buffers[frame % 3], expected, "scale={scale}, frame={frame}");
            tracker.submitted();
        }
    }
}

#[test]
fn patches_validate_before_mutation_and_invalidate_descendants() {
    let mut scene = Scene::default();
    let mut root = Element::new(0, fixed(20.0, 20.0));
    let mut item = child(1);
    item.style.position = taffy::Position::Absolute;
    item.offset = (40.0, 50.0);
    root.children.push(item);
    scene
        .update(root, (120, 120), 1.0, RecordingMode::Retained)
        .unwrap();
    let old = scene.bounds(0.into()).unwrap();
    assert_eq!(
        old.paint,
        Rect {
            x: 40,
            y: 50,
            width: 62,
            height: 42
        }
    );
    for invalid in [
        Change::Offset(99.into(), (1.0, 0.0)),
        Change::Offset(1.into(), (f32::NAN, 0.0)),
    ] {
        assert!(
            scene
                .patch([Change::Offset(0.into(), (5.0, 0.0)), invalid])
                .is_err()
        );
        assert_eq!(scene.bounds(0.into()).unwrap().layout, old.layout);
    }
    let damage = scene.patch([Change::Offset(0.into(), (5.0, 0.0))]).unwrap();
    assert_eq!(
        damage,
        Damage::rect(Rect {
            x: 40,
            y: 50,
            width: 67,
            height: 42
        })
    );
    assert_eq!(scene.stats.recordings, 0);
    assert_eq!(scene.stats.layout_computations, 0);
    assert_eq!(scene.hit_test((47.0, 52.0)), Some(1.into()));
}

#[test]
fn patches_keep_key_indices_until_display_or_topology_changes() {
    let mut scene = Scene::default();
    for hidden in [false, true, false] {
        let mut root = Element::row(0, 0.0, vec![child(1), child(2)]);
        if hidden {
            root.children[0].style.display = taffy::Display::None;
            root.children.reverse();
        }
        scene
            .update(root, (160, 100), 1.25, RecordingMode::Retained)
            .unwrap();
        let indices = scene.geometry_index.clone();
        for offset in [0.0, -90.0, 10.0] {
            scene
                .patch([
                    Change::Offset(0.into(), (offset, 0.0)),
                    Change::Paint(2.into(), None),
                ])
                .unwrap();
            assert_eq!(scene.geometry_index, indices);
            for geometry in scene.geometry() {
                assert_eq!(scene.bounds(geometry.key).unwrap().layout, geometry.layout);
            }
            assert_eq!(scene.bounds(1.into()).is_none(), hidden);
        }
    }
}

#[test]
fn custom_layout_nests_stacks_and_places_anchors_without_inflating_measurement() {
    fn anchor(context: &mut LayoutContext<'_>) -> Size<f32> {
        let size = context.container_size(Size {
            width: 100.0,
            height: 60.0,
        });
        let child = context.measure(0, size);
        context.place(
            0,
            Point {
                x: size.width - child.width - 4.0,
                y: size.height - child.height - 4.0,
            },
            child,
        );
        size
    }
    let row = Element::row(
        100,
        2.0,
        vec![
            Element::new(101, fixed(8.0, 6.0)),
            Element::new(102, fixed(8.0, 6.0)),
        ],
    );
    let mut anchored = Element::new(103, fixed(100.0, 60.0));
    anchored.algorithm = Algorithm::Custom(anchor);
    anchored.children.push(row);
    let root = Element::column(
        104,
        3.0,
        vec![anchored, Element::new(105, fixed(100.0, 10.0))],
    );
    let mut scene = Scene::default();
    scene
        .update(root, (150, 100), 1.25, RecordingMode::Retained)
        .unwrap();
    let bounds = |key| {
        scene
            .geometry()
            .iter()
            .find(|node| node.key == Key::from(key))
            .unwrap()
            .layout
    };
    assert_eq!(bounds(100), Bounds::new(78.0, 50.0, 18.0, 6.0));
    assert_eq!(bounds(105).y, 63.0);
}

#[test]
fn animation_is_coordinated_retargetable_and_keyed_not_indexed() {
    let now = Instant::now();
    let mut motion = SelectionMotion::new(4);
    motion.select(7, now, false);
    let mid = now + Duration::from_millis(85);
    assert_eq!(motion.weight(4, mid), 0.5);
    assert_eq!(motion.weight(7, mid), 0.5);
    motion.select(9, mid, false);
    assert_eq!(motion.weight(4, mid), 0.5);
    assert_eq!(motion.weight(7, mid), 0.5);
    let next = mid + Duration::from_millis(85);
    assert_eq!(
        [4, 7, 9]
            .map(|key| motion.weight(key, next))
            .iter()
            .sum::<f32>(),
        1.0
    );
    motion.select(4, next, true);
    assert_eq!(motion.weight(4, next), 1.0);
    assert_eq!(motion.deadline(next), None);
}

#[test]
fn paint_only_change_and_hit_geometry_follow_motion() {
    let mut scene = Scene::default();
    scene
        .update(child(1), (100, 100), 1.25, RecordingMode::Retained)
        .unwrap();
    let mut moved = child(1);
    moved.offset = (20.0, 10.0);
    moved.paint = Some(fill(20, [50, 60, 70, 255]));
    scene
        .update(moved, (100, 100), 1.25, RecordingMode::Retained)
        .unwrap();
    assert_eq!(scene.stats.layout_computations, 0);
    assert_eq!(scene.stats.recordings, 1);
    assert_eq!(scene.hit_test((26.0, 13.0)), Some(1.into()));
    assert_eq!(scene.hit_test((5.0, 5.0)), None);
}

#[test]
fn duplicate_keys_are_rejected_before_mutating_the_tree() {
    let mut retained = Scene::default();
    retained
        .update(child(1), (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    assert!(
        retained
            .update(
                Element::row(2, 0.0, vec![child(1), child(1)]),
                (100, 100),
                1.0,
                RecordingMode::Retained
            )
            .is_err()
    );
    assert_eq!(
        retained.update(child(1), (100, 100), 1.0, RecordingMode::Retained),
        Ok(Damage::None)
    );
}

#[test]
fn hidden_elements_clear_old_pixels_and_stop_receiving_input() {
    let mut scene = Scene::default();
    scene
        .update(child(1), (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    let mut hidden = child(1);
    hidden.style.display = taffy::Display::None;
    assert_ne!(
        scene
            .update(hidden, (100, 100), 1.0, RecordingMode::Retained)
            .unwrap(),
        Damage::None
    );
    assert_eq!(scene.hit_test((1.0, 1.0)), None);
    assert_eq!(scene.stats.visible_chunks, 0);
}

#[test]
fn offscreen_changes_do_not_request_redraw_and_new_scale_rebuilds_physical_caches() {
    let mut scene = Scene::default();
    let mut offscreen = child(1);
    offscreen.offset.1 = 200.0;
    scene
        .update(offscreen, (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    let mut offscreen = child(1);
    offscreen.offset.1 = 200.0;
    offscreen.paint = Some(fill(500, [12, 34, 56, 255]));
    assert_eq!(
        scene
            .update(offscreen, (100, 100), 1.0, RecordingMode::Retained)
            .unwrap(),
        Damage::None
    );
    assert_eq!(
        scene
            .update(child(1), (125, 125), 1.25, RecordingMode::Retained)
            .unwrap(),
        Damage::Full
    );
    assert_eq!(scene.stats.recordings, 1);
}

#[test]
fn moving_virtual_window_does_not_leak_arena_nodes() {
    let mut scene = Scene::default();
    for page in 0..100 {
        let root = Element::column(
            0,
            2.0,
            (1 + page * 20..=20 + page * 20).map(child).collect(),
        );
        scene
            .update(root, (100, 100), 1.0, RecordingMode::Retained)
            .unwrap();
        assert_eq!(scene.stats.nodes, 21);
        assert!(scene.tree.nodes.len() <= 41);
    }
}

#[test]
fn fixed_leaf_measurement_and_anchoring_match_painted_geometry() {
    fn overlay(context: &mut LayoutContext<'_>) -> Size<f32> {
        let size = context.container_size(Size::ZERO);
        context.anchor(
            0,
            Bounds::new(2.0, 4.0, 78.0, 76.0),
            Point { x: 1.0, y: 1.0 },
            Point::ZERO,
        );
        size
    }
    let mut root = Element::new(2, fixed(82.0, 84.0));
    root.algorithm = Algorithm::Custom(overlay);
    root.children.push(child(1));
    let mut scene = Scene::default();
    scene
        .update(root, (100, 100), 1.25, RecordingMode::Retained)
        .unwrap();
    let leaf = scene
        .geometry()
        .iter()
        .find(|node| node.key == 1.into())
        .unwrap();
    assert_eq!(leaf.layout, Bounds::new(18.0, 38.0, 62.0, 42.0));
    assert_eq!(leaf.hit, leaf.paint);
}

#[test]
#[should_panic(expected = "prepare this scene generation")]
fn outdated_preparation_cannot_paint_new_scene_even_with_identical_chunk_counts() {
    let mut scene = Scene::default();
    let mut renderer = Renderer::new(4096);
    scene
        .update(child(1), (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    renderer.prepare(&scene, RasterMode::Explicit);
    scene
        .update(child(2), (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    render(
        &scene,
        &mut renderer,
        &mut vec![0; 40000],
        (100, 100),
        Damage::Full.bounds((100, 100)).unwrap(),
    );
}

#[test]
fn z_stack_uses_one_grid_cell_and_painter_order_for_hits() {
    let mut scene = Scene::default();
    let root = Element::stack(10, fixed(100.0, 100.0), vec![child(1), child(2)]);
    scene
        .update(root, (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(scene.hit_test((20.0, 20.0)), Some(2.into()));
    let a = scene
        .geometry()
        .iter()
        .find(|node| node.key == 1.into())
        .unwrap();
    let b = scene
        .geometry()
        .iter()
        .find(|node| node.key == 2.into())
        .unwrap();
    assert_eq!(a.layout, b.layout);
}

#[test]
fn zero_budget_prefers_replay_instead_of_allocating_an_uncacheable_raster() {
    let mut scene = Scene::default();
    let mut renderer = Renderer::new(0);
    let mut element = child(1);
    element.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
    scene
        .update(element, (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    renderer.prepare(&scene, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 0);
    assert_eq!(renderer.cache_stats().live_bytes, 0);
}

#[test]
fn visible_rasters_survive_placement_updates_above_retention_budget() {
    for scale in [1.0, 1.25, 2.0] {
        let mut root = Element::row(
            0,
            0.0,
            (1..=4)
                .map(|id| {
                    let mut item = child(id);
                    item.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
                    item
                })
                .collect(),
        );
        root.clip = true;
        let mut scene = Scene::default();
        scene
            .update(root, (600, 400), scale, RecordingMode::Retained)
            .unwrap();
        let mut renderer = Renderer::new(1);
        renderer.prepare(&scene, RasterMode::Explicit);
        assert_eq!(renderer.stats.prepared_rasters, 4);
        assert!(renderer.cache_stats().live_bytes > 1);
        assert_eq!(renderer.cache_stats().cached_bytes, 0);
        for step in 1..5 {
            scene
                .patch([Change::Offset(0.into(), (0.0, step as f32))])
                .unwrap();
            renderer.prepare(&scene, RasterMode::Explicit);
            assert_eq!(renderer.stats.prepared_rasters, 0);
        }
        renderer.clear_cache();
        assert_eq!(renderer.cache_stats().live_bytes, 0);
    }
}

#[test]
fn raster_identity_does_not_leak_across_scenes() {
    let mut renderer = Renderer::new(4096);
    for color in [[255, 0, 0, 255], [0, 255, 0, 255]] {
        let mut item = Element::new(1, fixed(10.0, 10.0));
        item.paint = Some(fill(1, color));
        item.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
        let mut scene = Scene::default();
        scene
            .update(item, (10, 10), 1.0, RecordingMode::Retained)
            .unwrap();
        renderer.prepare(&scene, RasterMode::Explicit);
        assert_eq!(renderer.stats.prepared_rasters, 1);
        let mut pixels = vec![0; 400];
        render(
            &scene,
            &mut renderer,
            &mut pixels,
            (10, 10),
            Rect {
                width: 10,
                height: 10,
                ..Rect::default()
            },
        );
        assert_eq!(&pixels[220..224], &[color[2], color[1], color[0], color[3]]);
    }
}

#[test]
fn image_arrival_and_clipped_glyphs_match_full_replay_during_scroll() {
    use crate::painter::{GlyphCache, RasterImage, load_overlay_font};
    use std::rc::Rc;
    let font = Rc::new(load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap());
    let images = [
        Rc::new(
            RasterImage::from_rgba(crate::image::RgbaImage::from_pixel(
                16,
                16,
                crate::image::Rgba([100, 40, 50, 128]),
            ))
            .unwrap(),
        ),
        Rc::new(
            RasterImage::from_rgba(crate::image::RgbaImage::from_pixel(
                16,
                16,
                crate::image::Rgba([40, 160, 90, 128]),
            ))
            .unwrap(),
        ),
    ];
    for scale in [1.0, 1.25, 2.0] {
        let size = ((100.0 * scale) as u32, (100.0 * scale) as u32);
        let make = |frame: usize| {
            let font = font.clone();
            let image = images[usize::from(frame >= 3)].clone();
            let mut child = Element::new(2, fixed(80.0, 60.0));
            child.offset = (12.25, -(frame as f32) * 2.75);
            child.paint = Some(Paint::new(900 + u64::from(frame >= 3), move |_| {
                vec![
                    Command::Fill {
                        bounds: Bounds::new(0.0, 0.0, 80.0, 60.0),
                        radius: 8.0,
                        color: Color::rgba(30, 40, 90, 200),
                    },
                    Command::Image {
                        bounds: Bounds::new(0.0, 0.0, 40.0, 40.0),
                        image: image.clone().into(),
                        fit: ImageFit::Stretch,
                    },
                    Command::Text {
                        align: TextAlignment::Top,
                        bounds: Bounds::new(3.0, 30.0, 75.0, 25.0),
                        size: 16.0,
                        text: "Blueprint".into(),
                        font: font.clone(),
                        color: Color::rgb(0xffffff),
                    },
                ]
            }));
            child.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
            let mut root = Element::column(1, 0.0, vec![child]);
            root.style = fixed(90.0, 80.0);
            root.clip = true;
            root
        };
        let mut scene = Scene::default();
        let mut renderer = Renderer::new(4096 * 16);
        let mut replay = Renderer::new(0);
        let mut cached = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut full = cached.clone();
        for frame in 0..8 {
            let damage = scene
                .update(make(frame), size, scale, RecordingMode::Retained)
                .unwrap();
            renderer.prepare(&scene, RasterMode::Explicit);
            replay.prepare(&scene, RasterMode::Replay);
            render(
                &scene,
                &mut renderer,
                &mut cached,
                size,
                damage.bounds(size).unwrap(),
            );
            render(
                &scene,
                &mut replay,
                &mut full,
                size,
                Damage::Full.bounds(size).unwrap(),
            );
            let max = cached
                .iter()
                .zip(&full)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(max <= 2, "scale={scale} frame={frame} max={max}");
        }
    }
}
