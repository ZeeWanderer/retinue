use super::*;
use crate::{ResidencyPolicy, Translation};

#[test]
fn sparse_placement_visits_only_its_instance_and_groups() {
    let mut frame = PaintFrame::default();
    let mut definitions = PropertyDefinitions::default();
    let mut instances = Vec::new();
    let mut groups = Vec::new();
    for i in 0..128 {
        let id = i as u64 + 1;
        let mut instance = item(id, 0, (0, 0), 1.0);
        instance.placement = PaintPlacement::Properties(PropertyBinding {
            transform: Some(TransformId(id.into())),
            clip: None,
        });
        instances.push(instance);
        groups.push(PaintGroup {
            key: (id + 1000).into(),
            instances: i..i + 1,
            context: crate::OptimizationContext::default().with(CacheHint::Conditional),
        });
        definitions.translations.push(Translation {
            id: TransformId(id.into()),
            parent: None,
            offset: (10.0, 10.0),
        });
    }
    frame
        .replace_with_properties(SIZE, 1.0, instances, groups, definitions)
        .unwrap();
    let mut renderer = Renderer::with_layers(ResidencyPolicy::desktop(0), LayerPolicy::desktop());
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.planned_instances, 128);
    let before: Vec<_> = renderer.execution_plan().cloned().collect();
    frame
        .patch_properties([PropertyChange::Translation(
            TransformId(128.into()),
            (13.0, 12.0),
        )])
        .unwrap();
    let pixels = paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.inspected_instances, 1);
    assert_eq!(renderer.stats.planned_instances, 1);
    assert_eq!(renderer.stats.inspected_groups, 1);
    assert_eq!(renderer.stats.prepared_rasters, 0);
    assert_eq!(
        renderer.execution_plan().cloned().collect::<Vec<_>>(),
        before
    );
    assert_eq!(
        pixels,
        paint(
            &frame,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            RasterMode::Replay
        )
    );
}

#[test]
fn warm_item_returns_without_preparation_after_clip_reversal() {
    let mut frame = PaintFrame::default();
    let original = item(1, 0, (10, 10), 1.0);
    frame
        .replace(SIZE, 1.0, vec![original.clone()], Vec::new())
        .unwrap();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(65536));
    let before = paint(&frame, &mut renderer, RasterMode::Explicit);
    let mut hidden = original.clone();
    hidden.set_pixels((10, 10), Rect::default());
    frame.replace(SIZE, 1.0, vec![hidden], Vec::new()).unwrap();
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.residency_stats().selected_bytes, 0);
    assert!(renderer.residency_stats().warm_bytes > 0);
    frame
        .replace(SIZE, 1.0, vec![original], Vec::new())
        .unwrap();
    assert_eq!(before, paint(&frame, &mut renderer, RasterMode::Explicit));
    assert_eq!(renderer.stats.prepared_rasters, 0);
    assert_eq!(renderer.residency_stats().warm_bytes, 0);
}

#[test]
fn capacity_rejection_retries_after_old_native_readers_finish() {
    let mut policy = ResidencyPolicy::desktop(0);
    policy.peak_bytes = 40 * 30 * 4;
    let mut renderer = Renderer::new(policy);
    let mut frame = PaintFrame::default();
    frame
        .replace(SIZE, 1.0, vec![item(1, 0, (10, 10), 1.0)], Vec::new())
        .unwrap();
    renderer.prepare(&frame, RasterMode::Explicit);
    let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
    let mut painter = Painter::with_threads(
        &mut pixels,
        SIZE.0,
        SIZE.1,
        std::num::NonZeroU32::new(2).unwrap(),
    )
    .unwrap();
    renderer.paint(&frame, &mut painter, viewport(), Color::rgb(0));
    frame
        .replace(SIZE, 1.0, vec![item(1, 1, (10, 10), 1.0)], Vec::new())
        .unwrap();
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.stats.rejected_rasters, 1);
    assert_eq!(renderer.residency_stats().retired_bytes, 4800);
    assert_eq!(renderer.residency_stats().selected_bytes, 0);
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(
        renderer.stats.planned_instances, 0,
        "no capacity change should cause no retry"
    );
    painter.finish().unwrap();
    let actual = paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 1);
    assert_eq!(renderer.residency_stats().retired_bytes, 0);
    assert_eq!(
        actual,
        paint(
            &frame,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            RasterMode::Replay
        )
    );
}

#[test]
fn obsolete_parent_does_not_block_replacement_at_peak_limit() {
    let mut policy = ResidencyPolicy::desktop(0);
    policy.peak_bytes = 40 * 30 * 4;
    let mut renderer = Renderer::with_layers(
        policy,
        LayerPolicy {
            stable_repaints: 1,
            min_work: 1,
            ..LayerPolicy::desktop()
        },
    );
    let mut frame = PaintFrame::default();
    let mut instance = item(1, 0, (10, 10), 1.0);
    instance.cache = CachePolicy::Never;
    let group = PaintGroup {
        key: 2.into(),
        instances: 0..1,
        context: crate::OptimizationContext::default().with(CacheHint::Stable),
    };
    frame
        .replace(SIZE, 1.0, vec![instance], vec![group.clone()])
        .unwrap();
    for _ in 0..4 {
        paint(&frame, &mut renderer, RasterMode::Explicit);
    }
    assert_eq!(renderer.layers.count(), 1);
    let mut changed = item(1, 1, (10, 10), 1.0);
    changed.cache = CachePolicy::Never;
    frame
        .replace(SIZE, 1.0, vec![changed], vec![group])
        .unwrap();
    let actual = paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.layers.count(), 1);
    assert_eq!(renderer.stats.preparation_tasks, 1);
    assert_eq!(renderer.stats.copied_pixels, 0);
    assert_eq!(renderer.residency_stats().live_bytes, 4800);
    assert_eq!(
        actual,
        paint(
            &frame,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            RasterMode::Replay
        )
    );
}

#[test]
fn hidden_and_reordered_items_keep_painter_order_with_zero_warm_storage() {
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    let mut direct = Renderer::new(ResidencyPolicy::desktop(0));
    let mut frame = PaintFrame::default();
    let a = item(1, 0, (10, 10), 1.0);
    let b = item(2, 1, (20, 15), 1.0);
    for step in 0..12 {
        let mut instances = if step % 3 == 0 {
            vec![a.clone(), b.clone()]
        } else {
            vec![b.clone(), a.clone()]
        };
        if step % 3 == 2 {
            instances[0].set_pixels((0, 0), Rect::default());
        }
        frame.replace(SIZE, 1.0, instances, Vec::new()).unwrap();
        assert_eq!(
            paint(&frame, &mut renderer, RasterMode::Explicit),
            paint(&frame, &mut direct, RasterMode::Replay)
        );
        if step % 3 == 1 {
            assert_eq!(renderer.stats.prepared_rasters, 0);
        }
        assert_eq!(renderer.residency_stats().warm_bytes, 0);
        assert!(renderer.residency_stats().live_bytes <= 9600);
    }
    frame.replace(SIZE, 1.0, Vec::new(), Vec::new()).unwrap();
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.residency_stats().live_bytes, 0);
    assert_eq!(renderer.execution_plan().len(), 0);
}
