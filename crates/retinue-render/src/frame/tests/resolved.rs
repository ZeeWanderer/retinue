use super::*;

#[test]
fn resolved_content_and_placement_share_damage_and_retain_unaffected_rasters() {
    for scale in [1.0, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        frame
            .replace(
                SIZE,
                scale,
                vec![item(1, 0, (12, 18), scale), item(2, 0, (90, 80), scale)],
                vec![],
            )
            .unwrap();
        let before = frame.snapshot();
        let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(1024 * 1024));
        let mut pixels = paint(&frame, &mut renderer, RasterMode::Explicit);
        let damage = frame
            .patch_resolved(
                [PaintUpdate {
                    index: 0,
                    program: item(1, 1, (0, 0), scale).program,
                    cache: CachePolicy::Prefer,
                }],
                [PlacementUpdate {
                    index: 0,
                    origin: (-3, 22),
                    clip: Rect {
                        x: 8,
                        y: 14,
                        width: 48,
                        height: 25,
                    },
                }],
                [],
            )
            .unwrap();
        assert_eq!(frame.generation, before.frame().generation + 1);
        assert_eq!(frame.changes.instances, [0]);
        assert!(!frame.changes.topology);
        assert_eq!(before.frame().instances()[0].origin(), (12, 18));
        renderer.prepare(&frame, RasterMode::Explicit);
        assert_eq!(renderer.stats.inspected_instances, 1);
        let mut painter = Painter::new(&mut pixels, SIZE.0, SIZE.1).unwrap();
        for &rect in damage.regions(SIZE).rects() {
            renderer.paint(&frame, &mut painter, rect, Color::rgb(0x121419));
        }
        painter.finish().unwrap();
        let mut direct = Renderer::new(crate::ResidencyPolicy::desktop(0));
        direct.set_optimizations(false);
        assert_eq!(pixels, paint(&frame, &mut direct, RasterMode::Replay));
        let generation = frame.generation;
        assert_eq!(
            frame
                .patch_resolved(
                    [],
                    [PlacementUpdate {
                        index: 1,
                        origin: (90, 80),
                        clip: Rect {
                            width: 1000,
                            height: 1000,
                            ..Rect::default()
                        },
                    }],
                    []
                )
                .unwrap(),
            Damage::None
        );
        assert_eq!(frame.generation, generation);
    }
}

#[test]
fn resolved_patch_validates_all_updates_before_mutation() {
    let mut frame = PaintFrame::default();
    frame
        .replace(SIZE, 1.0, vec![item(1, 0, (10, 10), 1.0)], vec![])
        .unwrap();
    let generation = frame.generation;
    for indices in [vec![0, 1], vec![0, 0]] {
        let result = frame.patch_resolved(
            [PaintUpdate {
                index: 0,
                program: item(1, 1, (0, 0), 1.0).program,
                cache: CachePolicy::Never,
            }],
            indices.into_iter().map(|index| PlacementUpdate {
                index,
                origin: (20, 30),
                clip: viewport(),
            }),
            [],
        );
        assert!(result.is_err());
        assert_eq!(frame.generation, generation);
        assert_eq!(frame.instances()[0].origin(), (10, 10));
        assert_eq!(frame.instances()[0].program.key().revision, 0);
    }
    frame
        .replace_with_properties(
            SIZE,
            1.0,
            vec![PaintInstance::with_properties(
                1.into(),
                item(1, 0, (0, 0), 1.0).program,
                PropertyBinding::default(),
            )],
            vec![],
            PropertyDefinitions::default(),
        )
        .unwrap();
    let generation = frame.generation;
    assert!(
        frame
            .patch_resolved(
                [],
                [PlacementUpdate {
                    index: 0,
                    origin: (20, 30),
                    clip: viewport(),
                }],
                []
            )
            .is_err()
    );
    assert_eq!(frame.generation, generation);
}

#[test]
fn group_metadata_shares_the_placement_journal_and_validates_atomically() {
    let mut frame = PaintFrame::default();
    frame
        .replace(
            SIZE,
            1.0,
            vec![item(1, 0, (0, 0), 1.0)],
            vec![PaintGroup {
                key: "collection".into(),
                instances: 0..1,
                context: OptimizationContext::default(),
            }],
        )
        .unwrap();
    let generation = frame.generation;
    let placement = || PlacementUpdate {
        index: 0,
        origin: (5, 8),
        clip: viewport(),
    };
    assert!(
        frame
            .patch_resolved(
                [],
                [placement()],
                [GroupUpdate {
                    index: 0,
                    context: OptimizationContext::default().with(CollectionContext {
                        rows: vec![crate::CollectionRow {
                            key: 0.into(),
                            instances: 0..2
                        }],
                        ..CollectionContext::default()
                    }),
                }]
            )
            .is_err()
    );
    assert_eq!(frame.generation, generation);
    assert_eq!(frame.instances()[0].origin(), (0, 0));
    frame
        .patch_resolved(
            [],
            [placement()],
            [GroupUpdate {
                index: 0,
                context: OptimizationContext::default().with(CacheHint::Conditional),
            }],
        )
        .unwrap();
    assert_eq!(frame.generation, generation + 1);
    assert_eq!(frame.changes.instances, [0]);
    assert_eq!(frame.changes.groups, [0]);
    assert!(!frame.changes.topology);
}

#[test]
fn deferred_publications_keep_deltas_until_prepare_with_bounded_full_fallback() {
    let mut frame = PaintFrame::default();
    frame
        .replace(
            SIZE,
            1.0,
            (1..=3)
                .map(|id| item(id, 0, (id as i32 * 30, 10), 1.0))
                .collect(),
            vec![],
        )
        .unwrap();
    let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(1024 * 1024));
    renderer.prepare(&frame, RasterMode::Explicit);
    for revision in 1..=3 {
        frame
            .patch_resolved(
                [PaintUpdate {
                    index: 0,
                    program: item(1, revision, (0, 0), 1.0).program,
                    cache: CachePolicy::Prefer,
                }],
                [],
                [],
            )
            .unwrap();
    }
    frame
        .patch_resolved(
            [],
            [PlacementUpdate {
                index: 1,
                origin: (40, 30),
                clip: viewport(),
            }],
            [],
        )
        .unwrap();
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.stats.inspected_instances, 2);
    assert_eq!(renderer.stats.changed_programs, 1);
    for revision in 4..=12 {
        frame
            .patch_resolved(
                [PaintUpdate {
                    index: 0,
                    program: item(1, revision, (0, 0), 1.0).program,
                    cache: CachePolicy::Prefer,
                }],
                [],
                [],
            )
            .unwrap();
    }
    assert!(frame.history.len() <= 4);
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.stats.inspected_instances, 3);
    let mut direct = Renderer::new(crate::ResidencyPolicy::desktop(0));
    direct.set_optimizations(false);
    assert_eq!(
        paint(&frame, &mut renderer, RasterMode::Explicit),
        paint(&frame, &mut direct, RasterMode::Replay)
    );
}
