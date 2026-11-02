use super::*;
use crate::{OptimizationContext, ProgramStore, ResidencyPolicy};

fn glow(color: Color) -> PaintProgram {
    PaintProgram::new(
        77,
        (40.0, 40.0),
        1.0,
        Rc::new(vec![Command::InsetShadow {
            bounds: Bounds::new(0.0, 0.0, 40.0, 40.0),
            radius: 20.0,
            blur: 4.0,
            spread: 1.0,
            color,
        }]),
    )
    .unwrap()
}

fn fragment(program: PaintProgram) -> Command {
    Command::Fragment {
        program,
        context: OptimizationContext::default().with(CacheHint::Stable),
    }
}

fn populate(
    frame: &mut PaintFrame,
    store: &mut ProgramStore,
    scale: f32,
    cached: bool,
    outside: bool,
    changed: bool,
) {
    let mut items = Vec::new();
    let count = if outside { 5 } else { 4 };
    for index in 0..count {
        let color = if changed && index == 0 {
            Color::rgba(210, 60, 40, 220)
        } else {
            Color::rgba(70, 210, 52, 255)
        };
        let program = store
            .record(
                index as u64 + 1,
                (64.0, 60.0),
                scale,
                Rc::new(vec![
                    Command::Fill {
                        bounds: Bounds::new(0.0, 0.0, 64.0, 60.0),
                        radius: 4.0,
                        color: Color::rgb(0x202830 + index as u32),
                    },
                    Command::Translate {
                        offset: (12.0, 10.0),
                        commands: Rc::new(vec![fragment(glow(color))]),
                    },
                ]),
            )
            .unwrap();
        let clip = Rect {
            width: (350.0 * scale) as u32,
            height: (110.0 * scale) as u32,
            ..Rect::default()
        };
        let origin = (
            ((8.0 + index as f32 * 67.0) * scale).round() as i32,
            (13.0 * scale).round() as i32,
        );
        let mut item = PaintInstance::new((index as u64 + 1).into(), program, origin, clip);
        item.cache = if cached && index < 4 {
            CachePolicy::Prefer
        } else {
            CachePolicy::Never
        };
        items.push(item);
    }
    frame
        .replace(
            ((350.0 * scale) as u32, (110.0 * scale) as u32),
            scale,
            items,
            Vec::new(),
        )
        .unwrap();
}

fn draw(frame: &PaintFrame, renderer: &mut Renderer) -> Vec<u8> {
    let size = frame.size();
    let mut pixels = vec![0; (size.0 * size.1 * 4) as usize];
    renderer.prepare(frame, RasterMode::Explicit);
    let mut painter = Painter::new(&mut pixels, size.0, size.1).unwrap();
    renderer.paint(
        frame,
        &mut painter,
        Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        },
        Color::rgb(0x10151b),
    );
    painter.finish().unwrap();
    pixels
}

fn check(frame: &PaintFrame, renderer: &mut Renderer) {
    let mut raw = Renderer::new(ResidencyPolicy::desktop(0));
    raw.set_optimizations(false);
    let a = draw(frame, renderer);
    let b = draw(frame, &mut raw);
    let error = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(error <= 3, "scale {}: error {error}", frame.scale());
}

#[test]
fn repeated_source_is_shared_and_clean_frames_do_not_prepare_it_again() {
    for scale in [1.0, 1.2, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        populate(
            &mut frame,
            &mut ProgramStore::default(),
            scale,
            false,
            false,
            false,
        );
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        check(&frame, &mut renderer);
        assert_eq!(renderer.stats.prepared_fragments, 1);
        assert_eq!(renderer.stats.fragment_blits, 4);
        assert_eq!(renderer.fragments.selected.len(), 1);
        check(&frame, &mut renderer);
        assert_eq!(renderer.stats.preparation_tasks, 0);
        assert_eq!(renderer.stats.fragment_candidates, 0);
        assert_eq!(renderer.stats.fragment_blits, 4);
        assert!(matches!(
            frame.instances()[0].program.calls()[0].program.commands()[0],
            Command::InsetShadow { .. }
        ));
    }
}

#[test]
fn parents_consume_fragment_tasks_and_only_outside_users_keep_selected_pixels() {
    for outside in [false, true] {
        let mut frame = PaintFrame::default();
        populate(
            &mut frame,
            &mut ProgramStore::default(),
            1.25,
            true,
            outside,
            false,
        );
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        check(&frame, &mut renderer);
        assert_eq!(renderer.stats.prepared_fragments, 1);
        assert_eq!(renderer.stats.preparation_tasks, 5);
        assert_eq!(renderer.stats.preparation_edges, 4);
        assert_eq!(renderer.fragments.selected.len(), usize::from(outside));
        assert_eq!(
            renderer.residency_stats().selected_bytes,
            4 * 80 * 75 * 4 + usize::from(outside) * 50 * 50 * 4
        );
        check(&frame, &mut renderer);
        assert_eq!(renderer.stats.preparation_tasks, 0);
        frame
            .replace(frame.size(), frame.scale(), Vec::new(), Vec::new())
            .unwrap();
        renderer.prepare(&frame, RasterMode::Explicit);
        assert_eq!(renderer.residency_stats().live_bytes, 0);
    }
}

#[test]
fn source_versions_share_until_changed_and_old_snapshots_stay_valid() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(65536));
    populate(&mut frame, &mut store, 1.0, false, false, false);
    check(&frame, &mut renderer);
    let old = frame.snapshot();
    populate(&mut frame, &mut store, 1.0, false, false, true);
    check(&frame, &mut renderer);
    // A single inexpensive new fragment may replay instead of allocating.
    assert_eq!(renderer.fragments.selected.len(), 1);
    check(
        &old.frame(),
        &mut Renderer::new(ResidencyPolicy::desktop(0)),
    );
    populate(&mut frame, &mut store, 2.0, false, false, false);
    check(&frame, &mut renderer);
    assert_eq!(renderer.stats.prepared_fragments, 1);
}

#[test]
fn rejected_fragments_replay_and_unknown_context_cannot_change_pixels() {
    let mut frame = PaintFrame::default();
    populate(
        &mut frame,
        &mut ProgramStore::default(),
        1.0,
        false,
        false,
        false,
    );
    let mut policy = ResidencyPolicy::desktop(0);
    policy.max_surface_bytes = 100;
    let mut renderer = Renderer::new(policy);
    check(&frame, &mut renderer);
    assert_eq!(renderer.stats.prepared_fragments, 0);
    assert!(renderer.fragments.selected.is_empty());
    #[derive(Debug, PartialEq, Eq)]
    struct FuturePass(u32);
    frame
        .replace(
            frame.size(),
            frame.scale(),
            frame.instances().to_vec(),
            vec![PaintGroup {
                key: 55.into(),
                instances: 0..4,
                context: OptimizationContext::default().with(FuturePass(7)),
            }],
        )
        .unwrap();
    check(&frame, &mut renderer);
    frame
        .annotate_group(
            55.into(),
            OptimizationContext::default().with(FuturePass(8)),
        )
        .unwrap();
    check(&frame, &mut renderer);
    assert_eq!(renderer.stats.inspected_instances, 0);
}

#[test]
fn nested_fragments_keep_clips_and_outside_consumers_during_motion() {
    for scale in [1.0, 1.25, 2.0] {
        let leaf = glow(Color::rgba(70, 210, 52, 255));
        let parent = PaintProgram::new(
            88,
            (60.0, 55.0),
            scale,
            Rc::new(vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 60.0, 55.0),
                    radius: 3.0,
                    color: Color::rgba(40, 60, 90, 120),
                },
                Command::Clip {
                    bounds: Bounds::new(3.25, 4.5, 35.0, 41.0),
                    commands: Rc::new(vec![Command::Translate {
                        offset: (8.25, 9.5),
                        commands: Rc::new(vec![fragment(leaf.clone())]),
                    }]),
                },
            ]),
        )
        .unwrap();
        let mut store = ProgramStore::default();
        let clip = Rect {
            x: 2,
            y: 5,
            width: 330,
            height: 200,
        };
        let mut instances: Vec<_> = [parent.clone(), parent, leaf]
            .into_iter()
            .enumerate()
            .map(|(i, program)| {
                let program = store
                    .record(
                        i as u64,
                        (60.0, 60.0),
                        scale,
                        Rc::new(vec![fragment(program)]),
                    )
                    .unwrap();
                PaintInstance::new((i as u64).into(), program, (-7 + 70 * i as i32, 10), clip)
            })
            .collect();
        let mut frame = PaintFrame::default();
        frame
            .replace((350, 220), scale, instances.clone(), Vec::new())
            .unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        check(&frame, &mut renderer);
        assert_eq!(renderer.stats.prepared_fragments, 2);
        assert_eq!(renderer.fragments.selected.len(), 2);
        for delta in [-6, 0, 7, 24] {
            for (i, item) in instances.iter_mut().enumerate() {
                item.set_pixels((-7 + delta + 70 * i as i32, 10), clip);
            }
            frame
                .replace((350, 220), scale, instances.clone(), Vec::new())
                .unwrap();
            check(&frame, &mut renderer);
            assert_eq!(renderer.stats.preparation_tasks, 0);
        }
        instances.pop();
        frame
            .replace((350, 220), scale, instances, Vec::new())
            .unwrap();
        check(&frame, &mut renderer);
        assert_eq!(renderer.fragments.selected.len(), 1);
    }
}

#[test]
fn external_paint_inside_a_named_fragment_remains_a_barrier() {
    let native = PaintProgram::new(
        1,
        (20.0, 20.0),
        1.0,
        Rc::new(vec![Command::External {
            key: 9.into(),
            bounds: Bounds::new(0.0, 0.0, 20.0, 20.0),
        }]),
    )
    .unwrap();
    let program = PaintProgram::new(2, (20.0, 20.0), 1.0, Rc::new(vec![fragment(native)])).unwrap();
    let clip = Rect {
        width: 100,
        height: 30,
        ..Rect::default()
    };
    let mut frame = PaintFrame::default();
    frame
        .replace(
            (100, 30),
            1.0,
            (0..4)
                .map(|i| PaintInstance::new((i as u64).into(), program.clone(), (i * 25, 0), clip))
                .collect(),
            Vec::new(),
        )
        .unwrap();
    assert!(frame.has_external_paint());
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.stats.preparation_tasks, 0);
    let mut pixels = vec![0; 100 * 30 * 4];
    let mut painter = Painter::new(&mut pixels, 100, 30).unwrap();
    let mut calls = 0;
    renderer.paint_with(&frame, &mut painter, clip, Color::rgb(0), |key, p| {
        assert_eq!(key, 9.into());
        calls += 1;
        p.fill_rounded_rect(0, 0, 20, 20, 0, Color::rgb(0x884422).bgra());
    });
    painter.finish().unwrap();
    assert_eq!(calls, 4);
}
