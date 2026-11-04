use super::*;
use crate::damage::Region;

#[cfg(feature = "benchmark")]
mod benchmark;

fn unhinted(frame: &mut PaintFrame, store: &mut ProgramStore, scale: f32, count: usize) {
    let clip = Rect {
        width: ((350.0f32).max(count.min(12) as f32 * 67.0 + 16.0) * scale) as u32,
        height: ((110.0f32).max(count.div_ceil(12) as f32 * 67.0 + 26.0) * scale) as u32,
        ..Rect::default()
    };
    let items = (0..count)
        .map(|index| {
            let commands = Rc::new(vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 64.0, 60.0),
                    radius: 4.0,
                    color: Color::rgb(0x202830 + index as u32),
                },
                Command::Clip {
                    bounds: Bounds::new(0.0, 0.0, 64.0, 60.0),
                    commands: Rc::new(vec![Command::Translate {
                        offset: (-2.25, 10.35),
                        commands: Rc::new(vec![Command::InsetShadow {
                            bounds: Bounds::new(0.25, -0.25, 40.0, 40.0),
                            radius: 20.0,
                            blur: 4.0,
                            spread: 1.0,
                            color: Color::rgba(70, 210, 52, 210),
                        }]),
                    }]),
                },
            ]);
            let program = store
                .record(index as u64 + 1, (64.0, 60.0), scale, commands.clone())
                .unwrap();
            assert!(program.commands() == &commands);
            PaintInstance::new(
                (index as u64 + 1).into(),
                program,
                (
                    ((8.0 + (index % 12) as f32 * 67.0) * scale).round() as i32,
                    ((13.0 + (index / 12) as f32 * 67.0) * scale).round() as i32,
                ),
                clip,
            )
        })
        .collect();
    frame
        .replace((clip.width, clip.height), scale, items, Vec::new())
        .unwrap();
}

#[test]
fn unhinted_independent_views_share_expensive_drawing_and_preserve_source() {
    for scale in [1.0, 1.2, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        unhinted(&mut frame, &mut ProgramStore::default(), scale, 4);
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        check(&frame, &mut renderer);
        assert_eq!(renderer.stats.prepared_fragments, 1);
        assert_eq!(renderer.stats.fragment_blits, 4);
        assert_eq!(renderer.fragments.selected.len(), 1);
        renderer.prepare(&frame, RasterMode::Explicit);
        assert_eq!(renderer.stats.preparation_tasks, 0);
        assert_eq!(renderer.stats.fragment_candidates, 0);
    }
}

#[test]
fn authored_scopes_preserve_automatic_discovery_of_nested_drawing() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    unhinted(&mut frame, &mut store, 1.25, 4);
    let items = frame
        .instances()
        .iter()
        .cloned()
        .map(|mut item| {
            item.program = store
                .record(
                    item.program.key(),
                    item.program.size(),
                    1.25,
                    Rc::new(vec![Command::Fragment {
                        program: item.program,
                        context: OptimizationContext::default(),
                    }]),
                )
                .unwrap();
            item
        })
        .collect();
    frame
        .replace(frame.size(), frame.scale(), items, Vec::new())
        .unwrap();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    check(&frame, &mut renderer);
    assert_eq!(renderer.stats.prepared_fragments, 1);
    assert_eq!(renderer.stats.fragment_blits, 4);
}

#[test]
fn single_moving_candidate_requires_actual_repaints_not_idle_prepares() {
    let mut frame = PaintFrame::default();
    unhinted(&mut frame, &mut ProgramStore::default(), 1.25, 1);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    for _ in 0..20 {
        renderer.prepare(&frame, RasterMode::Explicit);
    }
    assert!(renderer.fragments.selected.is_empty());
    for step in 0..4 {
        let mut items = frame.instances().to_vec();
        let clip = items[0].clip();
        items[0].set_pixels((10 + step, 16), clip);
        frame
            .replace(frame.size(), frame.scale(), items, Vec::new())
            .unwrap();
        draw(&frame, &mut renderer);
        assert_eq!(renderer.fragments.selected.len(), usize::from(step == 3));
        if step == 0 {
            for _ in 0..20 {
                renderer.prepare(&frame, RasterMode::Explicit);
            }
            assert!(renderer.fragments.selected.is_empty());
        }
    }
    check(&frame, &mut renderer);
}

#[test]
fn relocation_does_not_create_or_expire_automatic_rasters() {
    let mut frame = PaintFrame::default();
    unhinted(&mut frame, &mut ProgramStore::default(), 1.0, 4);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    for _ in 0..20 {
        renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
    }
    assert!(renderer.fragments.selected.is_empty());
    check(&frame, &mut renderer);
    let selected = renderer.fragments.selected.clone();
    for step in 0..20 {
        let mut items = frame.instances().to_vec();
        let clip = items[0].clip();
        items[0].set_pixels((8, step), clip);
        frame
            .replace(frame.size(), frame.scale(), items, Vec::new())
            .unwrap();
        renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
        assert_eq!(renderer.fragments.selected, selected);
        assert_eq!(renderer.stats.preparation_tasks, 0);
    }
}

#[test]
fn cached_parents_consume_shared_discovery_without_duplicate_storage() {
    let mut frame = PaintFrame::default();
    unhinted(&mut frame, &mut ProgramStore::default(), 1.0, 4);
    let mut items = frame.instances().to_vec();
    for item in &mut items {
        item.cache = CachePolicy::Prefer;
    }
    frame
        .replace(frame.size(), frame.scale(), items, Vec::new())
        .unwrap();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    check(&frame, &mut renderer);
    assert_eq!(renderer.stats.preparation_tasks, 5);
    assert_eq!(renderer.stats.preparation_edges, 4);
    assert_eq!(renderer.stats.fragment_blits, 4);
    assert!(renderer.fragments.selected.is_empty());
    assert_eq!(renderer.residency_stats().selected_bytes, 4 * 64 * 60 * 4);
}

#[test]
fn automatic_rasters_are_released_when_content_is_removed() {
    let mut frame = PaintFrame::default();
    unhinted(&mut frame, &mut ProgramStore::default(), 1.0, 4);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    draw(&frame, &mut renderer);
    frame
        .replace(frame.size(), frame.scale(), Vec::new(), Vec::new())
        .unwrap();
    renderer.prepare(&frame, RasterMode::Explicit);
    assert!(renderer.fragments.selected.is_empty());
    assert_eq!(renderer.residency_stats().live_bytes, 0);
}

#[test]
fn multiple_repair_regions_do_not_count_as_several_frames() {
    let mut frame = PaintFrame::default();
    unhinted(&mut frame, &mut ProgramStore::default(), 1.0, 1);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.prepare(&frame, RasterMode::Replay);
    let size = frame.size();
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut p = Painter::new(&mut pixels, size.0, size.1).unwrap();
    for x in 0..40 {
        renderer.paint(
            &frame,
            &mut p,
            Rect {
                x,
                y: 0,
                width: 1,
                height: size.1,
            },
            Color::rgb(0x10151b),
        );
    }
    p.finish().unwrap();
    renderer.prepare(&frame, RasterMode::Replay);
    assert!(renderer.fragments.selected.is_empty());
}
