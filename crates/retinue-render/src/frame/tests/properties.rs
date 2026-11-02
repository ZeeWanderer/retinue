use super::*;
use crate::{Clip, Translation};

fn transform(id: u64) -> TransformId {
    TransformId(id.into())
}

fn clip_id(id: u64) -> ClipId {
    ClipId(id.into())
}

fn definitions() -> PropertyDefinitions {
    PropertyDefinitions {
        translations: vec![
            Translation {
                id: transform(2),
                parent: Some(transform(1)),
                offset: (8.6, 5.8),
            },
            Translation {
                id: transform(1),
                parent: None,
                offset: (2.2, 3.4),
            },
            Translation {
                id: transform(3),
                parent: None,
                offset: (80.0, 65.0),
            },
        ],
        clips: vec![
            Clip {
                id: clip_id(2),
                parent: Some(clip_id(1)),
                transform: Some(transform(2)),
                bounds: Bounds::new(0.0, 0.0, 47.0, 38.0),
            },
            Clip {
                id: clip_id(1),
                parent: None,
                transform: Some(transform(1)),
                bounds: Bounds::new(0.0, 0.0, 95.0, 80.0),
            },
        ],
    }
}

fn instances(scale: f32) -> Vec<PaintInstance> {
    (1..=3)
        .map(|id| {
            let mut instance = item(id, 0, (0, 0), scale);
            instance.placement = PaintPlacement::Properties(PropertyBinding {
                transform: Some(transform(if id < 3 { 2 } else { 3 })),
                clip: (id < 3).then(|| clip_id(2)),
            });
            instance
        })
        .collect()
}

fn frame(scale: f32) -> PaintFrame {
    let mut frame = PaintFrame::default();
    frame
        .replace_with_properties(SIZE, scale, instances(scale), Vec::new(), definitions())
        .unwrap();
    frame
}

#[test]
fn content_and_properties_patch_as_one_frame_and_repair_like_fresh_replay() {
    for scale in [1.0, 1.25, 2.0] {
        let mut frame = frame(scale);
        let old = frame.snapshot();
        let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(1024 * 1024));
        let mut pixels = paint(&frame, &mut renderer, RasterMode::Explicit);
        let program = PaintProgram::new(
            PaintKey::new(1.into(), 1),
            (40.0, 30.0),
            scale,
            Rc::new(vec![Command::Fill {
                bounds: Bounds::new(-6.0, -3.0, 22.0, 17.0),
                radius: 0.0,
                color: Color::rgb(0xdab321),
            }]),
        )
        .unwrap();
        let damage = frame
            .patch(
                [
                    PaintUpdate {
                        index: 0,
                        program,
                        cache: CachePolicy::Prefer,
                    },
                    PaintUpdate {
                        index: 2,
                        program: item(3, 1, (0, 0), scale).program,
                        cache: CachePolicy::Prefer,
                    },
                ],
                [
                    PropertyChange::Translation(transform(1), (12.7, 22.2)),
                    PropertyChange::ClipBounds(clip_id(2), Bounds::new(-8.0, -5.0, 29.2, 22.6)),
                ],
            )
            .unwrap();
        assert_eq!(frame.generation, old.frame().generation + 1);
        assert_eq!(frame.changes.from, old.frame().generation);
        assert_eq!(frame.changes.instances, [0, 1, 2]);
        assert!(!frame.changes.topology);
        assert_eq!(old.frame().instances()[0].program.key().revision, 0);
        renderer.prepare(&frame, RasterMode::Explicit);
        let mut painter = Painter::new(&mut pixels, SIZE.0, SIZE.1).unwrap();
        for rect in damage.regions(SIZE).rects() {
            renderer.paint(&frame, &mut painter, *rect, Color::rgb(0x121419));
        }
        painter.finish().unwrap();
        let mut direct = Renderer::new(crate::ResidencyPolicy::desktop(0));
        direct.set_optimizations(false);
        assert_eq!(
            pixels,
            paint(&frame, &mut direct, RasterMode::Replay),
            "scale={scale}"
        );
        assert_eq!(renderer.stats.inspected_instances, 3);
    }
}

#[test]
fn content_changes_recompute_bounds_without_property_changes() {
    let mut frame = frame(1.0);
    let old = frame.instances()[0].visible_bounds();
    let program = PaintProgram::new(
        4,
        (8.0, 6.0),
        1.0,
        Rc::new(vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, 8.0, 6.0),
            radius: 0.0,
            color: Color::rgb(0xffffff),
        }]),
    )
    .unwrap();
    let damage = frame
        .patch(
            [PaintUpdate {
                index: 0,
                program,
                cache: CachePolicy::Never,
            }],
            [],
        )
        .unwrap();
    assert_ne!(old, frame.instances()[0].visible_bounds());
    assert_eq!(
        frame.instances()[0].visible,
        frame.instances()[0].visible_bounds()
    );
    assert_eq!(frame.changes.instances, [0]);
    assert_eq!(damage, Damage::rect(old));
}

#[test]
fn invalid_content_or_properties_leave_the_entire_frame_unchanged() {
    let mut frame = frame(1.0);
    let before = frame.snapshot();
    let changed = item(1, 1, (0, 0), 1.0).program;
    let update = |index| PaintUpdate {
        index,
        program: changed.clone(),
        cache: CachePolicy::Never,
    };
    let move_root = || [PropertyChange::Translation(transform(1), (50.0, 60.0))];
    assert!(frame.patch([update(0), update(99)], move_root()).is_err());
    assert!(frame.patch([update(0), update(0)], move_root()).is_err());
    assert!(
        frame
            .patch(
                [PaintUpdate {
                    index: 0,
                    program: item(1, 1, (0, 0), 2.0).program,
                    cache: CachePolicy::Never
                }],
                move_root()
            )
            .is_err()
    );
    assert!(
        frame
            .patch(
                [update(0)],
                [PropertyChange::Translation(transform(99), (50.0, 60.0))]
            )
            .is_err()
    );
    assert_eq!(frame.generation, before.frame().generation);
    assert_eq!(
        frame.translation_revision(transform(1)),
        before.frame().translation_revision(transform(1))
    );
    for (actual, old) in frame.instances().iter().zip(before.frame().instances()) {
        assert_eq!(actual.program.id(), old.program.id());
        assert_eq!(actual.visible, old.visible);
        assert_eq!(actual.cache, old.cache);
    }
}

#[test]
fn identical_content_is_a_noop_and_cache_policy_does_not_damage_pixels() {
    let mut frame = frame(1.0);
    let generation = frame.generation;
    let program = frame.instances()[0].program.clone();
    assert_eq!(
        frame
            .patch(
                [PaintUpdate {
                    index: 0,
                    program: program.clone(),
                    cache: CachePolicy::Prefer
                }],
                []
            )
            .unwrap(),
        Damage::None
    );
    assert_eq!(frame.generation, generation);
    assert_eq!(
        frame
            .patch(
                [PaintUpdate {
                    index: 0,
                    program,
                    cache: CachePolicy::Never
                }],
                []
            )
            .unwrap(),
        Damage::None
    );
    assert_eq!(frame.generation, generation + 1);
    assert_eq!(frame.changes.instances, [0]);
}

#[test]
fn shared_properties_patch_only_consumers_and_reuse_rasters() {
    for scale in [1.0, 1.25, 2.0] {
        let mut frame = frame(scale);
        let old = frame.snapshot();
        let programs: Vec<_> = frame.instances().iter().map(|i| i.program.id()).collect();
        let unrelated = frame.translation_revision(transform(3));
        let mut cached = Renderer::new(crate::ResidencyPolicy::desktop(1024 * 1024));
        paint(&frame, &mut cached, RasterMode::Explicit);
        let damage = frame
            .patch_properties([
                PropertyChange::Translation(transform(1), (-4.7, 16.2)),
                PropertyChange::ClipBounds(clip_id(2), Bounds::new(2.1, 1.2, 29.2, 22.6)),
            ])
            .unwrap();
        assert!(damage != Damage::None);
        assert_eq!(frame.changes.instances, vec![0, 1]);
        assert_eq!(frame.property_stats().translations, 2);
        assert_eq!(frame.property_stats().clips, 2);
        assert_eq!(frame.property_stats().instances, 2);
        assert_eq!(frame.translation_revision(transform(3)), unrelated);
        assert_eq!(
            frame
                .instances()
                .iter()
                .map(|i| i.program.id())
                .collect::<Vec<_>>(),
            programs
        );
        assert_eq!(
            old.frame()
                .translation_revision(transform(1))
                .unwrap()
                .version,
            0
        );
        assert_eq!(frame.translation_revision(transform(1)).unwrap().version, 1);

        let origin = (-4.7_f32 + 8.6, 16.2_f32 + 5.8);
        let parent_clip = Bounds::new(-4.7, 16.2, 95.0, 80.0)
            .pixels(scale)
            .visible((0, 0), viewport());
        let clip = Bounds::new(origin.0 + 2.1, origin.1 + 1.2, 29.2, 22.6)
            .pixels(scale)
            .visible((0, 0), parent_clip);
        let mut direct_items = frame.instances().to_vec();
        for instance in &mut direct_items[..2] {
            instance.set_pixels(
                (
                    (origin.0 * scale).round() as i32,
                    (origin.1 * scale).round() as i32,
                ),
                clip,
            );
        }
        direct_items[2].set_pixels(((80.0 * scale) as i32, (65.0 * scale) as i32), viewport());
        let mut direct = PaintFrame::default();
        direct
            .replace(SIZE, scale, direct_items, Vec::new())
            .unwrap();
        assert_eq!(
            paint(&frame, &mut cached, RasterMode::Explicit),
            paint(
                &direct,
                &mut Renderer::new(crate::ResidencyPolicy::desktop(0)),
                RasterMode::Replay
            )
        );
        assert_eq!(cached.stats.prepared_rasters, 0);
        assert_eq!(cached.stats.inspected_instances, 2);
        assert_eq!(cached.stats.changed_programs, 0);
    }
}

#[test]
fn clip_changes_hide_and_restore_consumers_without_changing_programs() {
    let mut frame = frame(1.25);
    let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(1024 * 1024));
    let before = paint(&frame, &mut renderer, RasterMode::Explicit);
    frame
        .patch_properties([PropertyChange::ClipBounds(clip_id(2), Bounds::default())])
        .unwrap();
    assert!(frame.instances()[0].visible_bounds().is_empty());
    assert_ne!(before, paint(&frame, &mut renderer, RasterMode::Explicit));
    frame
        .patch_properties([PropertyChange::ClipBounds(
            clip_id(2),
            definitions().clips[0].bounds,
        )])
        .unwrap();
    assert_eq!(before, paint(&frame, &mut renderer, RasterMode::Explicit));
    assert_eq!(renderer.stats.prepared_rasters, 0);
}

#[test]
fn subpixel_updates_change_dependencies_not_pixel_damage() {
    let mut frame = frame(1.0);
    let old = frame.translation_revision(transform(3)).unwrap();
    let generation = frame.generation;
    let damage = frame
        .patch_properties([PropertyChange::Translation(transform(3), (80.1, 65.1))])
        .unwrap();
    assert_eq!(damage, Damage::None);
    assert!(frame.changes.instances.is_empty());
    assert_eq!(frame.generation, generation + 1);
    let updated = frame.translation_revision(transform(3)).unwrap();
    assert_eq!(updated.identity, old.identity);
    assert_eq!(updated.version, old.version + 1);
    let generation = frame.generation;
    assert_eq!(
        frame
            .patch_properties([
                PropertyChange::Translation(transform(3), (80.4, 65.4)),
                PropertyChange::Translation(transform(3), (80.1, 65.1)),
            ])
            .unwrap(),
        Damage::None
    );
    assert_eq!(frame.generation, generation);
    assert_eq!(frame.property_stats().translations, 0);
}

#[test]
fn bad_patches_are_atomic_including_overflow_after_composition() {
    let mut frame = frame(2.0);
    let before = frame.snapshot();
    for invalid in [
        PropertyChange::Translation(transform(99), (1.0, 1.0)),
        PropertyChange::Translation(transform(1), (f32::NAN, 1.0)),
        PropertyChange::Translation(transform(1), (i32::MAX as f32, 1.0)),
        PropertyChange::ClipBounds(clip_id(99), Bounds::default()),
        PropertyChange::ClipBounds(clip_id(1), Bounds::new(0.0, 0.0, -1.0, 1.0)),
        PropertyChange::ClipBounds(clip_id(1), Bounds::new(0.0, 0.0, f32::MAX, 1.0)),
    ] {
        assert!(
            frame
                .patch_properties([
                    PropertyChange::Translation(transform(3), (50.0, 50.0)),
                    invalid,
                ])
                .is_err()
        );
        assert_eq!(frame.generation, before.frame().generation);
        assert_eq!(
            frame.translation_revision(transform(3)),
            before.frame().translation_revision(transform(3))
        );
        assert_eq!(frame.chunks[2].origin(), before.frame().chunks[2].origin());
    }
}

#[test]
fn property_topology_and_bindings_are_validated_before_replacement() {
    let mut frame = frame(1.0);
    let generation = frame.generation;
    let mut duplicate = definitions();
    duplicate.translations.push(duplicate.translations[0]);
    let mut cycle = definitions();
    cycle.translations[1].parent = Some(transform(2));
    let mut clip_cycle = definitions();
    clip_cycle.clips[1].parent = Some(clip_id(2));
    let mut missing = definitions();
    missing.clips[0].transform = Some(transform(99));
    let mut missing_parent = definitions();
    missing_parent.translations[1].parent = Some(transform(99));
    for definitions in [
        duplicate,
        cycle,
        clip_cycle,
        missing,
        missing_parent,
        PropertyDefinitions::default(),
    ] {
        assert!(
            frame
                .replace_with_properties(SIZE, 1.0, instances(1.0), Vec::new(), definitions)
                .is_err()
        );
        assert_eq!(frame.generation, generation);
        assert_eq!(frame.instances().len(), 3);
    }
}

#[test]
fn replacement_preserves_unchanged_revisions_but_reused_keys_get_new_lifetimes() {
    let mut frame = frame(1.0);
    let first = frame.translation_revision(transform(1));
    let snapshot = frame.snapshot();
    frame
        .replace_with_properties(SIZE, 1.0, frame.chunks.clone(), Vec::new(), definitions())
        .unwrap();
    assert_eq!(frame.translation_revision(transform(1)), first);
    assert_eq!(frame.generation, snapshot.frame().generation);
    frame.replace(SIZE, 1.0, Vec::new(), Vec::new()).unwrap();
    assert_eq!(frame.translation_revision(transform(1)), None);
    frame
        .replace_with_properties(SIZE, 1.0, instances(1.0), Vec::new(), definitions())
        .unwrap();
    assert_ne!(frame.translation_revision(transform(1)), first);
    assert_eq!(snapshot.frame().translation_revision(transform(1)), first);
}

#[test]
fn offscreen_property_coordinates_do_not_overflow_visible_bounds() {
    let mut frame = frame(1.0);
    frame
        .patch_properties([PropertyChange::Translation(
            transform(3),
            (2_147_483_520.0, 0.0),
        )])
        .unwrap();
    assert!(frame.instances()[2].visible_bounds().is_empty());
    assert!(
        crate::PaintExtent {
            x: i32::MAX,
            y: 0,
            width: 10,
            height: 10
        }
        .visible((i32::MAX, 0), viewport())
        .is_empty()
    );
}
