use super::*;
use crate::{
    CollectionContext, CollectionMotion, OptimizationContext, Partition, ProgramStore,
    ResidencyPolicy, TileHint,
};

fn collection(
    frame: &mut PaintFrame,
    store: &mut ProgramStore,
    scale: f32,
    scroll: (i32, i32),
    changed: bool,
) {
    let px = |v: f32| (v * scale).round() as u32;
    let clip = Rect {
        x: px(20.0),
        y: px(20.0),
        width: px(160.0),
        height: px(100.0),
    };
    let origin = (clip.x as i32 - scroll.0, clip.y as i32 - scroll.1);
    let items = (0..20)
        .map(|i| {
            let program = store
                .record(
                    PaintKey::new((i as u64).into(), u64::from(changed && i == 5)),
                    (64.0, 48.0),
                    scale,
                    Rc::new(vec![
                        Command::Fill {
                            bounds: Bounds::new(-3.0, -2.0, 64.0, 48.0),
                            radius: 4.0,
                            color: Color::rgba(30, 70, 90 + (i * 7) as u8, 190),
                        },
                        Command::Fill {
                            bounds: Bounds::new(9.0, 10.0, 7.0, 6.0),
                            radius: 0.0,
                            color: Color::rgb(if changed && i == 5 {
                                0xffaf67
                            } else {
                                0x64b752
                            }),
                        },
                    ]),
                )
                .unwrap();
            PaintInstance::new(
                (i as u64).into(),
                program,
                (
                    origin.0 + px((i % 4) as f32 * 48.0) as i32,
                    origin.1 + px((i / 4) as f32 * 38.0) as i32,
                ),
                clip,
            )
        })
        .collect();
    frame
        .replace(
            (px(240.0), px(180.0)),
            scale,
            items,
            vec![PaintGroup {
                key: 100.into(),
                instances: 0..20,
                context: OptimizationContext::default()
                    .with(CacheHint::Stable)
                    .with(CollectionContext {
                        origin,
                        motion: CollectionMotion::Translating,
                        rows: Vec::new(),
                    })
                    .with(TileHint {
                        size: (px(64.0), px(48.0)),
                    }),
            }],
        )
        .unwrap();
}

fn renderer(warm: usize) -> Renderer {
    Renderer::with_layers(
        ResidencyPolicy::desktop(warm),
        LayerPolicy {
            stable_repaints: 1,
            conditional_repaints: 2,
            min_work: 1,
            threads: 2,
        },
    )
}

fn draw(frame: &PaintFrame, renderer: &mut Renderer) -> Vec<u8> {
    let size = frame.size();
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    renderer.prepare(frame, RasterMode::Explicit);
    let mut p = Painter::new(&mut pixels, size.0, size.1).unwrap();
    renderer.paint(
        frame,
        &mut p,
        Rect {
            width: size.0,
            height: size.1,
            ..Default::default()
        },
        Color::rgb(0x121419),
    );
    p.finish().unwrap();
    pixels
}

fn check(frame: &PaintFrame, cached: &mut Renderer) {
    let mut raw = Renderer::new(ResidencyPolicy::desktop(0));
    raw.set_optimizations(false);
    let actual = draw(frame, cached);
    let expected = draw(frame, &mut raw);
    let (index, error) = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| a.abs_diff(*b))
        .enumerate()
        .max_by_key(|(_, error)| *error)
        .unwrap();
    assert!(
        error <= 2,
        "tile pixel error {error} at {},{}, scale {}",
        index / 4 % frame.size().0 as usize,
        index / 4 / frame.size().0 as usize,
        frame.scale()
    );
}

fn warm(frame: &PaintFrame, cached: &mut Renderer) {
    for _ in 0..18 {
        check(frame, cached);
    }
}

#[test]
fn spatial_tiles_and_source_replay_share_a_range_without_translucent_seams() {
    for scale in [1.0, 1.2, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        collection(
            &mut frame,
            &mut ProgramStore::default(),
            scale,
            (0, 0),
            false,
        );
        let mut cached = renderer(0);
        check(&frame, &mut cached);
        check(&frame, &mut cached);
        assert_eq!(cached.layers.count(), 1);
        assert!(
            cached
                .execution_plan()
                .any(|s| matches!(s, crate::DrawStep::Clipped { raster: false, .. }))
        );
        warm(&frame, &mut cached);
        assert_eq!(cached.layers.count(), 9);
        assert!(cached.execution_plan().all(|s| matches!(s, crate::DrawStep::Layer { group, .. } if matches!(group.partition, Partition::Tile { .. }))));
        check(&frame, &mut cached);
        assert_eq!(cached.stats.preparation_tasks, 0);
        assert_eq!(cached.residency_stats().warm_bytes, 0);
    }
}

#[test]
fn scrolling_tiles_repair_edges_and_small_content_changes_together() {
    for scale in [1.0, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        let mut store = ProgramStore::default();
        let mut cached = renderer(512 * 1024);
        collection(&mut frame, &mut store, scale, (0, 0), false);
        warm(&frame, &mut cached);
        let snapshot = frame.snapshot();
        let bytes = cached.residency_stats().selected_bytes;
        collection(&mut frame, &mut store, scale, (3, 4), true);
        check(&frame, &mut cached);
        assert!(cached.stats.repaired_pixels > 0);
        assert!(cached.stats.repaired_pixels * 4 < bytes / 2);
        for scroll in [(8, 12), (30, 60), (60, 95), (30, 60), (-9, -7), (0, 0)] {
            collection(&mut frame, &mut store, scale, scroll, true);
            for _ in 0..4 {
                check(&frame, &mut cached);
            }
        }
        let mut old = renderer(0);
        warm(snapshot.frame(), &mut old);
        frame
            .replace(frame.size(), scale, Vec::new(), Vec::new())
            .unwrap();
        cached.prepare(&frame, RasterMode::Explicit);
        assert_eq!(cached.residency_stats().live_bytes, 0);
    }
}

#[test]
fn denied_tiles_keep_remaining_regions_in_source_order() {
    let mut frame = PaintFrame::default();
    collection(
        &mut frame,
        &mut ProgramStore::default(),
        1.25,
        (0, 0),
        false,
    );
    let tile_bytes = 80 * 60 * 4;
    let mut cached = Renderer::with_layers(
        ResidencyPolicy {
            peak_bytes: tile_bytes,
            max_surface_bytes: tile_bytes,
            ..ResidencyPolicy::desktop(0)
        },
        LayerPolicy {
            stable_repaints: 1,
            conditional_repaints: 1,
            min_work: 1,
            threads: 2,
        },
    );
    warm(&frame, &mut cached);
    assert_eq!(cached.layers.count(), 1);
    assert_eq!(cached.residency_stats().selected_bytes, tile_bytes);
    assert!(
        cached
            .execution_plan()
            .any(|s| matches!(s, crate::DrawStep::Clipped { .. }))
    );
    assert!(cached.stats.rejected_rasters > 0);
}

#[test]
fn parent_consumes_partial_child_tiles_and_replays_the_gaps() {
    let mut frame = PaintFrame::default();
    collection(
        &mut frame,
        &mut ProgramStore::default(),
        1.25,
        (0, 0),
        false,
    );
    let mut cached = renderer(0);
    check(&frame, &mut cached);
    check(&frame, &mut cached);
    assert_eq!(cached.layers.count(), 1);
    let mut groups = frame.groups().to_vec();
    groups.insert(
        0,
        PaintGroup {
            key: 1.into(),
            instances: 0..20,
            context: OptimizationContext::default().with(CacheHint::Stable),
        },
    );
    frame
        .replace(
            frame.size(),
            frame.scale(),
            frame.instances().to_vec(),
            groups,
        )
        .unwrap();
    check(&frame, &mut cached);
    check(&frame, &mut cached);
    assert!(cached.stats.preparation_blits > 0);
    assert_eq!(cached.layers.count(), 1);
    assert_eq!(
        cached.layers.prepared.keys().next().unwrap(),
        &crate::ScopeId::group(1.into())
    );
    assert_eq!(cached.residency_stats().entries, 1);
}

#[test]
fn tile_context_changes_and_reorder_cannot_change_paint() {
    let mut frame = PaintFrame::default();
    collection(
        &mut frame,
        &mut ProgramStore::default(),
        1.25,
        (0, 0),
        false,
    );
    let mut cached = renderer(0);
    warm(&frame, &mut cached);
    let context = frame.groups()[0].context.clone();
    let mut collection = frame.groups()[0].collection().unwrap().clone();
    collection.motion = CollectionMotion::Reordering;
    frame
        .annotate_group(100.into(), context.clone().with(collection))
        .unwrap();
    check(&frame, &mut cached);
    assert_eq!(cached.layers.count(), 0);
    assert_eq!(cached.stats.inspected_instances, 0);
    for size in [(0, 32), (1, 1), (u32::MAX, u32::MAX), (80, 60)] {
        frame
            .annotate_group(100.into(), context.clone().with(TileHint { size }))
            .unwrap();
        warm(&frame, &mut cached);
        assert!(cached.stats.inspected_regions <= 256);
    }
    let mut collection = frame.groups()[0].collection().unwrap().clone();
    collection.origin = (i32::MIN, i32::MAX);
    frame
        .annotate_collection(100.into(), Some(collection))
        .unwrap();
    check(&frame, &mut cached);
}

#[test]
fn later_overlay_preserves_paint_order_above_tiles_and_partial_item_rasters() {
    let mut frame = PaintFrame::default();
    collection(
        &mut frame,
        &mut ProgramStore::default(),
        1.25,
        (0, 0),
        false,
    );
    let mut instances = frame.instances().to_vec();
    for item in &mut instances {
        item.cache = CachePolicy::Prefer;
    }
    let program = PaintProgram::new(
        900,
        (110.0, 50.0),
        1.25,
        Rc::new(vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, 110.0, 50.0),
            radius: 5.0,
            color: Color::rgba(220, 70, 40, 160),
        }]),
    )
    .unwrap();
    instances.push(PaintInstance::new(
        900.into(),
        program,
        (32, 44),
        Rect {
            width: frame.size().0,
            height: frame.size().1,
            ..Default::default()
        },
    ));
    frame
        .replace(
            frame.size(),
            frame.scale(),
            instances,
            frame.groups().to_vec(),
        )
        .unwrap();
    let mut cached = renderer(0);
    for _ in 0..3 {
        check(&frame, &mut cached);
    }
    assert!(
        cached
            .execution_plan()
            .any(|s| matches!(s, crate::DrawStep::Clipped { raster: true, .. }))
    );
    warm(&frame, &mut cached);
    assert!(matches!(
        cached.execution_plan().last(),
        Some(crate::DrawStep::Replay { instance: 20 })
    ));
}

#[test]
fn hidden_tiles_can_return_from_warm_storage_without_native_preparation() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let mut cached = renderer(512 * 1024);
    collection(&mut frame, &mut store, 1.25, (0, 0), false);
    warm(&frame, &mut cached);
    let bytes = cached.residency_stats().selected_bytes;
    collection(&mut frame, &mut store, 1.25, (0, 700), false);
    check(&frame, &mut cached);
    assert_eq!(cached.residency_stats().selected_bytes, 0);
    assert_eq!(cached.residency_stats().warm_bytes, bytes);
    let hits = cached.residency_stats().hits;
    collection(&mut frame, &mut store, 1.25, (0, 0), false);
    check(&frame, &mut cached);
    assert_eq!(cached.stats.preparation_tasks, 0);
    assert_eq!(cached.residency_stats().selected_bytes, bytes);
    assert!(cached.residency_stats().hits > hits);
}

#[test]
fn tile_boundaries_preserve_text_images_and_shadow_geometry() {
    use crate::painter::{GlyphCache, load_overlay_font};
    use crate::{ImageSource, TextAlignment};
    let font = Rc::new(load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap());
    for scale in [1.0, 1.2, 1.25, 2.0] {
        let px = |v: f32| (v * scale).round() as u32;
        let mut frame = PaintFrame::default();
        let mut cached = renderer(0);
        for revision in 0..2 {
            let image = Rc::new(
                RasterImage::render(16, 8, |p| {
                    p.fill_rounded_rect(0, 0, 16, 8, 0, Color::rgba(60, 130, 180, 230).bgra());
                    p.fill_rounded_rect(
                        5,
                        0,
                        3,
                        8,
                        0,
                        Color::rgb(if revision == 0 { 0xe0a040 } else { 0x90b4e0 }).bgra(),
                    );
                })
                .unwrap(),
            );
            let program = PaintProgram::new(
                PaintKey::new(1.into(), revision),
                (150.0, 90.0),
                scale,
                Rc::new(vec![
                    Command::Fill {
                        bounds: Bounds::new(0.0, 0.0, 150.0, 90.0),
                        radius: 0.0,
                        color: Color::rgb(0x243849),
                    },
                    Command::Image {
                        bounds: Bounds::new(7.0, 5.0, 112.0, 50.0),
                        image: ImageSource::Shared(image),
                        fit: ImageFit::Stretch,
                    },
                    Command::InsetShadow {
                        bounds: Bounds::new(18.0, 8.0, 90.0, 70.0),
                        radius: 35.0,
                        blur: 5.0,
                        spread: 1.0,
                        color: Color::rgb(0x60bf5c),
                    },
                    Command::Text {
                        bounds: Bounds::new(0.0, 24.0, 150.0, 42.0),
                        size: 19.0,
                        text: "Tile boundaries".into(),
                        font: font.clone(),
                        color: Color::rgb(0xffffff),
                        align: TextAlignment::Center,
                    },
                ]),
            )
            .unwrap();
            let size = (px(210.0), px(140.0));
            let origin = (px(20.0) as i32 + revision as i32, px(20.0) as i32);
            frame
                .replace(
                    size,
                    scale,
                    vec![PaintInstance::new(
                        1.into(),
                        program,
                        origin,
                        Rect {
                            width: size.0,
                            height: size.1,
                            ..Default::default()
                        },
                    )],
                    vec![PaintGroup {
                        key: 100.into(),
                        instances: 0..1,
                        context: OptimizationContext::default()
                            .with(CacheHint::Stable)
                            .with(CollectionContext {
                                origin,
                                ..Default::default()
                            })
                            .with(TileHint {
                                size: (px(48.0), px(32.0)),
                            }),
                    }],
                )
                .unwrap();
            warm(&frame, &mut cached);
            assert_eq!(cached.layers.count(), 12);
        }
    }
}

#[test]
fn tile_hint_cannot_capture_native_paint_or_duplicate_its_callback() {
    let mut frame = PaintFrame::default();
    collection(
        &mut frame,
        &mut ProgramStore::default(),
        1.25,
        (0, 0),
        false,
    );
    let mut items = frame.instances().to_vec();
    items[0].program = PaintProgram::new(
        990,
        (90.0, 60.0),
        1.25,
        Rc::new(vec![Command::External {
            bounds: Bounds::new(0.0, 0.0, 90.0, 60.0),
            key: 990.into(),
        }]),
    )
    .unwrap();
    frame
        .replace(frame.size(), frame.scale(), items, frame.groups().to_vec())
        .unwrap();
    let mut cached = renderer(0);
    let mut pixels = vec![0; frame.size().0 as usize * frame.size().1 as usize * 4];
    for _ in 0..3 {
        cached.prepare(&frame, RasterMode::Explicit);
        let mut calls = 0;
        let mut p = Painter::new(&mut pixels, frame.size().0, frame.size().1).unwrap();
        cached.paint_with(
            &frame,
            &mut p,
            Rect {
                width: frame.size().0,
                height: frame.size().1,
                ..Default::default()
            },
            Color::rgb(0),
            |_, _| calls += 1,
        );
        p.finish().unwrap();
        assert_eq!(calls, 1);
        assert_eq!(cached.layers.count(), 0);
    }
}
