use super::*;
use crate::damage::Region;

fn full(frame: &PaintFrame) -> Region {
    Region::new([Rect {
        width: frame.size().0,
        height: frame.size().1,
        ..Rect::default()
    }])
}

fn assert_current_pixels(frame: &PaintFrame, renderer: &mut Renderer) {
    let size = frame.size();
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut painter = Painter::new(&mut pixels, size.0, size.1).unwrap();
    renderer.paint(
        frame,
        &mut painter,
        full(frame).bounds(),
        Color::rgb(0x10151b),
    );
    painter.finish().unwrap();
    let mut raw = Renderer::new(ResidencyPolicy::desktop(0));
    raw.set_optimizations(false);
    let expected = draw(frame, &mut raw);
    let error = pixels
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(b))
        .max()
        .unwrap();
    assert!(error <= 3, "pixel error {error}");
}

#[test]
fn relocate_demand_keeps_live_shared_fragment_without_preparing_it_again() {
    let mut frame = PaintFrame::default();
    populate(
        &mut frame,
        &mut ProgramStore::default(),
        1.25,
        false,
        false,
        false,
    );
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    check(&frame, &mut renderer);
    let selected = renderer.fragments.selected.clone();
    let bytes = renderer.residency_stats().selected_bytes;
    let original = frame.instances().to_vec();
    for step in 0..64 {
        let items = original
            .iter()
            .cloned()
            .map(|mut item| {
                item.set_pixels((item.origin().0, item.origin().1 + step % 4), item.clip());
                item
            })
            .collect();
        frame
            .replace(frame.size(), frame.scale(), items, Vec::new())
            .unwrap();
        renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
        assert_eq!(renderer.fragments.selected, selected);
        assert_eq!(renderer.stats.preparation_tasks, 0);
        assert_eq!(renderer.residency_stats().selected_bytes, bytes);
        assert_eq!(renderer.residency_stats().warm_bytes, 0);
        assert!(!renderer.has_deferred());
    }
    renderer
        .residency
        .maintain(std::time::Instant::now() + std::time::Duration::from_secs(60));
    renderer.prepare_for(&frame, RasterMode::Explicit, &full(&frame));
    assert_eq!(renderer.fragments.selected, selected);
    assert_eq!(renderer.stats.preparation_tasks, 0);
    assert_current_pixels(&frame, &mut renderer);
    assert_eq!(renderer.stats.fragment_blits, 4);
}

#[test]
fn unseen_fragment_waits_for_paint_without_requiring_a_new_scene_revision() {
    let mut frame = PaintFrame::default();
    populate(
        &mut frame,
        &mut ProgramStore::default(),
        1.0,
        false,
        false,
        false,
    );
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    for _ in 0..4 {
        renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
        assert_eq!(renderer.stats.preparation_tasks, 0);
        assert_eq!(renderer.residency_stats().live_bytes, 0);
        assert!(!renderer.has_deferred());
    }
    renderer.prepare_for(&frame, RasterMode::Explicit, &full(&frame));
    assert_eq!(renderer.stats.prepared_fragments, 1);
    assert_current_pixels(&frame, &mut renderer);
    assert_eq!(renderer.stats.fragment_blits, 4);
}

#[test]
fn empty_repair_does_not_keep_obsolete_or_removed_consumers() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    populate(&mut frame, &mut store, 1.0, false, false, false);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(65536));
    check(&frame, &mut renderer);
    let selected = renderer.fragments.selected.clone();
    populate(&mut frame, &mut store, 1.0, false, false, true);
    renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
    assert_eq!(renderer.fragments.selected, selected);
    assert_eq!(renderer.stats.preparation_tasks, 0);
    assert_current_pixels(&frame, &mut renderer);
    frame
        .replace(frame.size(), frame.scale(), Vec::new(), Vec::new())
        .unwrap();
    renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
    assert!(renderer.fragments.selected.is_empty());
    assert_eq!(renderer.residency_stats().live_bytes, 0);
    assert!(!renderer.has_deferred());
}

#[test]
fn partial_item_demand_only_materializes_requested_consumers() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    populate(&mut frame, &mut store, 1.0, true, false, false);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
    assert_eq!(renderer.stats.preparation_tasks, 0);
    assert!(!renderer.has_deferred());
    let repair = Region::new([frame.instances()[0].visible_bounds()]);
    renderer.prepare_for(&frame, RasterMode::Explicit, &repair);
    assert_eq!(renderer.stats.prepared_rasters, 1);
    assert_current_pixels(&frame, &mut renderer);
    renderer.prepare_for(&frame, RasterMode::Explicit, &full(&frame));
    assert_eq!(renderer.stats.prepared_rasters, 3);
    assert_current_pixels(&frame, &mut renderer);
    populate(&mut frame, &mut store, 1.0, true, false, true);
    renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
    assert_eq!(renderer.stats.preparation_tasks, 0);
    assert!(!renderer.has_deferred());
    assert_current_pixels(&frame, &mut renderer);
    renderer.prepare_for(&frame, RasterMode::Explicit, &repair);
    assert_eq!(renderer.stats.prepared_rasters, 1);
    assert_current_pixels(&frame, &mut renderer);
}

fn group(frame: &mut PaintFrame) {
    frame
        .replace(
            frame.size(),
            frame.scale(),
            frame.instances().to_vec(),
            vec![PaintGroup {
                key: 100.into(),
                instances: 0..4,
                context: OptimizationContext::default().with(CacheHint::Stable),
            }],
        )
        .unwrap();
}

#[test]
fn unrequested_dirty_layer_stays_invalid_until_it_is_repaired() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    populate(&mut frame, &mut store, 1.0, false, false, false);
    group(&mut frame);
    let mut renderer = Renderer::with_layers(
        ResidencyPolicy::desktop(0),
        LayerPolicy {
            stable_repaints: 1,
            conditional_repaints: 2,
            min_work: 1,
            threads: 2,
        },
    );
    for _ in 0..4 {
        check(&frame, &mut renderer);
    }
    assert_eq!(renderer.layers.count(), 1);
    populate(&mut frame, &mut store, 1.0, false, false, true);
    group(&mut frame);
    renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
    assert_eq!(renderer.stats.preparation_tasks, 0);
    assert_eq!(renderer.layers.count(), 0);
    assert!(!renderer.has_deferred());
    assert_current_pixels(&frame, &mut renderer);
    renderer.prepare_for(&frame, RasterMode::Explicit, &full(&frame));
    assert_eq!(renderer.layers.count(), 1);
    assert_current_pixels(&frame, &mut renderer);
}
