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
            let commands = Arc::new(vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 64.0, 60.0),
                    radius: 4.0,
                    color: Color::rgb(0x202830 + index as u32),
                },
                Command::Clip {
                    bounds: Bounds::new(0.0, 0.0, 64.0, 60.0),
                    commands: Arc::new(vec![Command::Translate {
                        offset: (-2.25, 10.35),
                        commands: Arc::new(vec![Command::InsetShadow {
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
                    Arc::new(vec![Command::Fragment {
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
fn unchanged_partial_repair_does_not_retry_offscreen_fragments() {
    let mut f = PaintFrame::default();
    unhinted(&mut f, &mut ProgramStore::default(), 1.0, 4);
    let mut r = Renderer::new(ResidencyPolicy::desktop(0));
    r.set_leaf_packet_reuse(true);
    let outside = Rect {
        x: 340,
        y: 0,
        width: 5,
        height: 5,
    }
    .into();
    r.prepare_for(&f, RasterMode::Explicit, &outside);
    assert!(r.stats.fragment_candidates > 0);
    r.prepare_for(&f, RasterMode::Explicit, &outside);
    assert_eq!(r.stats.fragment_candidates, 0);
    drop(r.freeze(&f, &outside, |_, _| unreachable!()).unwrap());
    let shifted = outside.local((2, 0));
    r.prepare_for(&f, RasterMode::Explicit, &shifted);
    #[cfg(feature = "benchmark")]
    assert_eq!(r.leaf_packet_counts().unwrap()[2], 0);
    assert!(
        r.stats.fragment_candidates > 0,
        "changed demand must still be checked"
    );
    r.prepare(&f, RasterMode::Explicit);
    assert_eq!(r.stats.prepared_fragments, 1);
}

#[test]
fn retained_leaf_credits_completed_paint_not_freeze() {
    let mut frame = PaintFrame::default();
    unhinted(&mut frame, &mut ProgramStore::default(), 1.0, 1);
    let full: Region = frame.instances()[0].clip().into();
    let (w, h) = frame.size();
    let mut cached = Renderer::new(ResidencyPolicy::desktop(0));
    cached.set_leaf_packet_reuse(true);
    let mut ordinary = Renderer::new(ResidencyPolicy::desktop(0));
    ordinary.set_leaf_packet_reuse(false);
    let prepare = |r: &mut Renderer| {
        r.prepare_for(&frame, RasterMode::Explicit, &full);
        r.freeze(&frame, &full, |_, _| unreachable!()).unwrap()
    };
    let paint = |packet: &crate::DrawPacket, repair: &Region, fail: bool| {
        let mut pixels = vec![0; (w * h * 4) as usize];
        let mut p = Painter::new(&mut pixels, w, h).unwrap();
        p.repair(repair);
        let stats = packet.draw(&mut p);
        if fail {
            p.fail(crate::painter::PainterError::invalid("discard feedback"));
        }
        assert_eq!(p.finish().is_err(), fail);
        (pixels, stats.fragment_blits)
    };
    let outside: Region = Rect {
        x: w - 3,
        y: h - 3,
        width: 3,
        height: 3,
    }
    .into();
    for _ in 0..12 {
        drop(prepare(&mut cached));
        let packet = prepare(&mut cached);
        paint(&packet, &outside, false);
        paint(&packet, &Region::default(), false);
        paint(&packet, &full, true);
    }
    for _ in 0..12 {
        let a = prepare(&mut cached);
        let b = prepare(&mut ordinary);
        assert_eq!(
            ordinary.fragments.selected.len(),
            cached.fragments.selected.len()
        );
        assert_eq!(paint(&a, &full, false), paint(&b, &full, false));
    }
    assert_eq!(cached.fragments.selected.len(), 1);
    let old = prepare(&mut cached);
    cached.clear_cache();
    for _ in 0..12 {
        paint(&old, &full, false);
        drop(prepare(&mut cached));
    }
    assert!(cached.fragments.selected.is_empty());
    let first = prepare(&mut cached);
    for _ in 0..12 {
        paint(&first, &full, false);
    }
    let promoted = prepare(&mut cached);
    assert_eq!(paint(&promoted, &full, false).1, 1);
}

#[test]
fn swept_leaf_credits_only_fragments_in_the_presented_repair() {
    let mut frame = PaintFrame::default();
    unhinted(&mut frame, &mut ProgramStore::default(), 1., 1);
    let (w, h) = frame.size();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.set_leaf_packet_reuse(true);
    let mut pixels = vec![0; (w * h * 4) as usize];
    let mut present = |r: &mut Renderer, x| {
        let area = Region::from(Rect {
            x,
            y: 0,
            width: 96,
            height: h,
        });
        r.prepare_for(&frame, RasterMode::Explicit, &area);
        let packet = r.freeze(&frame, &area, |_, _| unreachable!()).unwrap();
        let mut p = Painter::new(&mut pixels, w, h).unwrap();
        p.repair(&area);
        let stats = packet.draw(&mut p);
        p.finish().unwrap();
        stats.fragment_blits
    };
    // The sweep reaches x=4, but the shadow near the left edge has not been drawn.
    for x in [100, 52].into_iter().cycle().take(20) {
        assert_eq!(present(&mut renderer, x), 0);
    }
    let blits = present(&mut renderer, 4);
    assert_eq!(blits, 0);
    assert!(renderer.fragments.selected.is_empty());
    for _ in 0..12 {
        present(&mut renderer, 4);
    }
    assert_eq!(present(&mut renderer, 4), 1);
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
    for step in 0..8 {
        let mut items = frame.instances().to_vec();
        let clip = items[0].clip();
        items[0].set_pixels((10 + step, 16), clip);
        frame
            .replace(frame.size(), frame.scale(), items, Vec::new())
            .unwrap();
        draw(&frame, &mut renderer);
        if step < 3 {
            assert!(renderer.fragments.selected.is_empty());
        }
        if step == 0 {
            for _ in 0..20 {
                renderer.prepare(&frame, RasterMode::Explicit);
            }
            assert!(renderer.fragments.selected.is_empty());
        }
    }
    assert_eq!(renderer.fragments.selected.len(), 1);
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
