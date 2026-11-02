use super::*;
use crate::{CollectionContext, CollectionMotion, CollectionRow, ResidencyPolicy};

fn collection(frame: &mut PaintFrame, scale: f32, scroll: i32, revision: u64) {
    let px = |n: f32| (n * scale).round() as u32;
    let viewport = Rect {
        x: px(20.0),
        y: px(25.0),
        width: px(110.0),
        height: px(64.0),
    };
    let origin = (viewport.x as i32, viewport.y as i32 - scroll);
    let mut items = Vec::new();
    for row in 0..3 {
        for column in 0..2 {
            let id = (row * 2 + column + 1) as u64;
            let mut instance = item(
                id,
                if id == 1 { revision } else { 0 },
                (
                    origin.0 + px(column as f32 * 55.0) as i32,
                    origin.1 + px(row as f32 * 35.0) as i32,
                ),
                scale,
            );
            instance.set_pixels(instance.origin(), viewport);
            if let Some(old) = frame.instances().iter().find(|old| {
                old.key == instance.key
                    && old.program.key() == instance.program.key()
                    && old.program.scale() == scale
            }) {
                instance.program = old.program.clone();
            }
            items.push(instance);
        }
    }
    frame
        .replace(
            (px(180.0), px(140.0)),
            scale,
            items,
            vec![PaintGroup {
                key: 100.into(),
                instances: 0..6,
                context: crate::OptimizationContext::default()
                    .with(CacheHint::Stable)
                    .with(CollectionContext {
                        origin,
                        motion: CollectionMotion::Translating,
                        rows: (0..3)
                            .map(|row| CollectionRow {
                                key: (row as u64).into(),
                                instances: row * 2..row * 2 + 2,
                            })
                            .collect(),
                    }),
            }],
        )
        .unwrap();
}

fn renderer() -> Renderer {
    Renderer::with_layers(
        ResidencyPolicy::desktop(256 * 1024),
        LayerPolicy {
            stable_repaints: 1,
            conditional_repaints: 2,
            min_work: 1,
            threads: 2,
        },
    )
}

fn draw(frame: &PaintFrame, renderer: &mut Renderer) -> Vec<u8> {
    let (width, height) = frame.size();
    let mut pixels = vec![0; width as usize * height as usize * 4];
    renderer.prepare(frame, RasterMode::Explicit);
    let mut painter = Painter::new(&mut pixels, width, height).unwrap();
    renderer.paint(
        frame,
        &mut painter,
        Rect {
            width,
            height,
            ..Rect::default()
        },
        Color::rgb(0x121419),
    );
    painter.finish().unwrap();
    pixels
}

fn check(frame: &PaintFrame, cached: &mut Renderer) {
    let mut direct = Renderer::new(ResidencyPolicy::desktop(0));
    direct.set_optimizations(false);
    let actual = draw(frame, cached);
    let expected = draw(frame, &mut direct);
    let error = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(error <= 2, "pixel error {error}, scale {}", frame.scale());
}

#[test]
fn content_rows_repair_scroll_edges_and_mutations_without_whole_row_repaint() {
    for scale in [1.0, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        let mut cached = renderer();
        collection(&mut frame, scale, 0, 0);
        for _ in 0..6 {
            check(&frame, &mut cached);
        }
        assert_eq!(cached.layers.count(), 2);
        let before = cached.residency_stats().selected_bytes;
        collection(&mut frame, scale, 3, 0);
        check(&frame, &mut cached);
        assert_eq!(cached.stats.preparation_tasks, 2);
        assert!(cached.stats.repaired_pixels < cached.stats.copied_pixels / 2);
        assert_eq!(cached.residency_stats().selected_bytes, before);
        for _ in 0..4 {
            check(&frame, &mut cached);
        }
        collection(&mut frame, scale, 5, 1);
        check(&frame, &mut cached);
        assert!(cached.stats.repaired_pixels < cached.stats.copied_pixels / 2);
        for scroll in [12, 25, 45, 90, 45, 12, 0, -10, 0] {
            collection(&mut frame, scale, scroll, 1);
            for _ in 0..4 {
                check(&frame, &mut cached);
            }
        }
        frame
            .replace(frame.size(), scale, Vec::new(), Vec::new())
            .unwrap();
        cached.prepare(&frame, RasterMode::Explicit);
        assert_eq!(cached.residency_stats().live_bytes, 0);
    }
}

#[test]
fn reordering_hint_changes_representation_without_changing_paint() {
    let mut frame = PaintFrame::default();
    let mut cached = renderer();
    collection(&mut frame, 1.25, 0, 0);
    for _ in 0..5 {
        check(&frame, &mut cached);
    }
    assert_eq!(cached.layers.count(), 2);
    let mut context = frame.groups()[0].collection().cloned().unwrap();
    context.motion = CollectionMotion::Reordering;
    frame
        .annotate_collection(100.into(), Some(context))
        .unwrap();
    check(&frame, &mut cached);
    assert!(!frame.changes.topology);
    assert_eq!(cached.stats.inspected_instances, 0);
    assert_eq!(cached.layers.count(), 0);
    assert_eq!(cached.stats.prepared_rasters, 4);
    let original = frame.instances().to_vec();
    for step in 1..8 {
        let items = original
            .iter()
            .enumerate()
            .map(|(i, old)| {
                let mut item = old.clone();
                item.set_pixels(
                    (
                        old.origin().0 + if i % 2 == 0 { step } else { -step },
                        old.origin().1,
                    ),
                    old.clip(),
                );
                item
            })
            .collect();
        frame
            .replace(frame.size(), frame.scale(), items, frame.groups().to_vec())
            .unwrap();
        check(&frame, &mut cached);
        assert_eq!(cached.stats.prepared_rasters, 0);
        assert_eq!(cached.layers.count(), 0);
    }
    check(&frame, &mut cached);
    assert_eq!(cached.stats.inspected_groups, 0);
    assert_eq!(cached.stats.inspected_regions, 0);
    assert_eq!(cached.stats.planned_instances, 0);
    let mut context = frame.groups()[0].collection().cloned().unwrap();
    context.motion = CollectionMotion::Steady;
    frame
        .annotate_collection(100.into(), Some(context))
        .unwrap();
    for _ in 0..6 {
        check(&frame, &mut cached);
    }
    assert_eq!(cached.layers.count(), 2);
}

#[test]
fn outer_motion_hint_updates_equal_range_collection_candidates() {
    let mut frame = PaintFrame::default();
    let mut cached = renderer();
    collection(&mut frame, 1.0, 0, 0);
    let mut groups = frame.groups().to_vec();
    groups.insert(
        0,
        PaintGroup {
            key: 99.into(),
            instances: 0..6,
            context: crate::OptimizationContext::default().with(CacheHint::Dynamic),
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
    for _ in 0..5 {
        check(&frame, &mut cached);
    }
    assert_eq!(cached.layers.count(), 2);
    frame
        .annotate_collection(
            99.into(),
            Some(CollectionContext {
                motion: CollectionMotion::Reordering,
                ..CollectionContext::default()
            }),
        )
        .unwrap();
    check(&frame, &mut cached);
    assert_eq!(cached.stats.inspected_instances, 0);
    assert_eq!(cached.layers.count(), 0);
    frame.annotate_collection(99.into(), None).unwrap();
    for _ in 0..5 {
        check(&frame, &mut cached);
    }
    assert_eq!(cached.layers.count(), 2);
}

#[test]
fn collection_hint_does_not_force_unprofitable_or_oversized_rasters() {
    let mut frame = PaintFrame::default();
    collection(&mut frame, 1.0, 0, 0);
    for oversized in [false, true] {
        let mut residency = ResidencyPolicy::desktop(0);
        if oversized {
            residency.max_surface_bytes = 40 * 30 * 4;
        }
        let mut cached = Renderer::with_layers(
            residency,
            LayerPolicy {
                stable_repaints: 1,
                min_work: if oversized { 1 } else { 1000 },
                ..LayerPolicy::desktop()
            },
        );
        for _ in 0..8 {
            check(&frame, &mut cached);
        }
        assert_eq!(cached.layers.count(), 0);
        assert!(cached.residency_stats().selected_bytes > 0);
        assert_eq!(cached.stats.prepared_rasters, 0);
    }
}

#[test]
fn invalid_collection_annotations_are_atomic() {
    let mut frame = PaintFrame::default();
    collection(&mut frame, 1.0, 0, 0);
    let original = frame.groups()[0].collection().cloned().unwrap();
    let version = frame.generation;
    for kind in 0..3 {
        let mut bad = original.clone();
        match kind {
            0 => bad.rows[1].key = bad.rows[0].key,
            1 => bad.rows[1].instances.start -= 1,
            _ => {
                bad.rows.pop();
            }
        }
        assert!(
            frame
                .annotate_collection(100.into(), Some(bad.clone()))
                .is_err()
        );
        let mut groups = frame.groups().to_vec();
        groups[0].context = groups[0].context.clone().with(bad);
        assert!(
            frame
                .replace(
                    frame.size(),
                    frame.scale(),
                    frame.instances().to_vec(),
                    groups
                )
                .is_err()
        );
        assert_eq!(frame.groups()[0].collection(), Some(&original));
        assert_eq!(frame.generation, version);
    }
}

#[test]
fn collection_motion_invalidates_cached_ancestors_even_with_simultaneous_paint_change() {
    let mut frame = PaintFrame::default();
    collection(&mut frame, 1.0, 0, 0);
    let parent = PaintGroup {
        key: 99.into(),
        instances: 0..6,
        context: crate::OptimizationContext::default().with(CacheHint::Stable),
    };
    let mut groups = vec![parent.clone()];
    groups.extend_from_slice(frame.groups());
    frame
        .replace(
            frame.size(),
            frame.scale(),
            frame.instances().to_vec(),
            groups,
        )
        .unwrap();
    let mut cached = renderer();
    for _ in 0..6 {
        check(&frame, &mut cached);
    }
    assert!(cached.execution_plan().any(|step| matches!(step,
        crate::DrawStep::Layer { group, .. } if group.group == 99.into())));
    let mut next = PaintFrame::default();
    collection(&mut next, 1.0, 0, 1);
    let mut groups = vec![parent];
    groups.extend_from_slice(next.groups());
    let mut collection = groups[1].collection().cloned().unwrap();
    collection.motion = CollectionMotion::Reordering;
    groups[1].context = groups[1].context.clone().with(collection);
    frame
        .replace(
            frame.size(),
            frame.scale(),
            next.instances().to_vec(),
            groups,
        )
        .unwrap();
    assert!(!frame.changes.topology);
    check(&frame, &mut cached);
    assert_eq!(cached.stats.inspected_groups, 2);
    assert_eq!(cached.layers.count(), 0);
}

#[test]
fn crossing_row_hints_fall_back_to_ordered_group_candidates() {
    let mut frame = PaintFrame::default();
    collection(&mut frame, 1.0, 0, 0);
    let mut other = frame.groups()[0].clone();
    other.key = 101.into();
    let mut context = other.collection().cloned().unwrap();
    context.rows = vec![
        CollectionRow {
            key: 0.into(),
            instances: 0..3,
        },
        CollectionRow {
            key: 1.into(),
            instances: 3..6,
        },
    ];
    other.context = other.context.with(context);
    let mut groups = frame.groups().to_vec();
    groups.push(other);
    frame
        .replace(
            frame.size(),
            frame.scale(),
            frame.instances().to_vec(),
            groups,
        )
        .unwrap();
    let mut cached = renderer();
    for _ in 0..6 {
        check(&frame, &mut cached);
    }
    assert_eq!(cached.layers.count(), 1);
    assert!(cached.execution_plan().all(|step| match step {
        crate::DrawStep::Layer { group, .. } => group.partition == crate::Partition::Whole,
        _ => true,
    }));
}

#[test]
fn invalid_coordinate_hints_cannot_change_source_paint() {
    let mut frame = PaintFrame::default();
    collection(&mut frame, 1.0, 0, 0);
    let mut cached = renderer();
    for _ in 0..6 {
        check(&frame, &mut cached);
    }
    let mut context = frame.groups()[0].collection().cloned().unwrap();
    context.origin = (i32::MIN, i32::MAX);
    frame
        .annotate_collection(100.into(), Some(context))
        .unwrap();
    check(&frame, &mut cached);
    assert_eq!(cached.layers.count(), 0);
}

#[test]
fn row_coverage_preserves_overlapping_translucent_paint_and_negative_scroll() {
    for scale in [1.0, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        let mut cached = renderer();
        for scroll in [0, 3, 14, 8, -6, 0] {
            collection(&mut frame, scale, scroll, 0);
            let items = frame
                .instances()
                .iter()
                .enumerate()
                .map(|(i, old)| {
                    let mut instance = old.clone();
                    instance.program = PaintProgram::new(
                        i as u64 + 1,
                        (40.0, 30.0),
                        scale,
                        Rc::new(vec![
                            Command::Fill {
                                bounds: Bounds::new(-4.0, -4.0, 48.0, 40.0),
                                radius: 9.0,
                                color: Color::rgba(50, (i * 21) as u8, 110, 160),
                            },
                            Command::InsetShadow {
                                bounds: Bounds::new(0.0, 0.0, 40.0, 30.0),
                                radius: 9.0,
                                blur: 4.0,
                                spread: 1.0,
                                color: Color::rgba(20, 200, 80, 180),
                            },
                        ]),
                    )
                    .unwrap();
                    instance
                })
                .collect();
            frame
                .replace(frame.size(), scale, items, frame.groups().to_vec())
                .unwrap();
            for _ in 0..6 {
                check(&frame, &mut cached);
            }
            assert!(cached.layers.count() > 0);
        }
    }
}
