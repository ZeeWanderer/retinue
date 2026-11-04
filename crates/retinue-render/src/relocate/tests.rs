use super::*;
use crate::{Bounds, CachePolicy, PaintInstance, PaintKey, ProgramStore, ResidencyPolicy};

fn renderer() -> Renderer {
    Renderer::new(ResidencyPolicy::desktop(0))
}

fn paint(target: &mut Target, frame: &PaintFrame, motion: Motion, reuse: bool) -> Sample {
    target
        .paint(
            frame,
            motion,
            Color::rgb(0x18212a),
            &mut renderer(),
            RasterMode::Replay,
            NonZeroU32::new(4).unwrap(),
            reuse,
        )
        .unwrap()
}

#[test]
fn overlapping_moves_match_snapshot_with_padded_stride() {
    let (width, height, stride) = (8, 7, 40);
    let original: Vec<_> = (0..stride * height).map(|i| (i * 31 % 251) as u8).collect();
    for w in 1..=width {
        for h in 1..=height {
            for sy in 0..=height - h {
                for sx in 0..=width - w {
                    for dy in 0..=height - h {
                        for dx in 0..=width - w {
                            let source = Rect {
                                x: sx as u32,
                                y: sy as u32,
                                width: w as u32,
                                height: h as u32,
                            };
                            let destination = Rect {
                                x: dx as u32,
                                y: dy as u32,
                                ..source
                            };
                            let mut actual = original.clone();
                            move_pixels(&mut actual, stride, source, destination);
                            let mut expected = original.clone();
                            for y in 0..h {
                                let from = (sy + y) * stride + sx * 4;
                                let to = (dy + y) * stride + dx * 4;
                                expected[to..to + w * 4]
                                    .copy_from_slice(&original[from..from + w * 4]);
                            }
                            assert_eq!(actual, expected, "{source:?} -> {destination:?}");
                        }
                    }
                }
            }
        }
    }
}

fn scene(frame: &mut PaintFrame, store: &mut ProgramStore, step: usize, motion: Motion) {
    let mut items = Vec::new();
    let size = (180, 140);
    let mut add = |id: u64, origin, bounds: Bounds, color, clip| {
        let program = store.compile(
            PaintKey::new(id.into(), 0),
            (bounds.width, bounds.height),
            1.0,
            Rc::new(vec![Command::Fill {
                bounds,
                radius: 0.0,
                color,
            }]),
        );
        items.push(PaintInstance::new(id.into(), program, origin, clip));
    };
    add(
        1,
        (0, 0),
        Bounds::new(0.0, 0.0, 180.0, 140.0),
        Color::rgba(20, 30, 40, 120),
        motion.viewport,
    );
    for i in 0..24 {
        if step >= 5 && i == 3 {
            continue;
        }
        let changed = step >= 3 && i == 4;
        let color = Color::rgba(if changed { 60 } else { 120 }, 90, 180, 200);
        add(
            i + 2,
            (
                motion.origin.0 + (i % 4) as i32 * 36,
                motion.origin.1 + (i / 4) as i32 * 24,
            ),
            Bounds::new(0.0, 0.0, 28.0, 20.0),
            color,
            motion.viewport,
        );
    }
    if step >= 4 {
        add(
            99,
            (motion.origin.0 + 20, motion.origin.1 + 40),
            Bounds::new(0.0, 0.0, 32.0, 22.0),
            Color::rgba(230, 80, 60, 170),
            motion.viewport,
        );
    }
    // Fixed, translucent foreground must not be moved with the scrolling content.
    add(
        100,
        (44, 48),
        Bounds::new(0.0, 0.0, 30.0, 17.0),
        Color::rgba(20, 220, 40, 160),
        motion.viewport,
    );
    add(
        101,
        (50, 50),
        Bounds::new(0.0, 0.0, 4.0, 4.0),
        Color::rgb(0xfefefe),
        motion.viewport,
    );
    if step >= 6 {
        items.swap(1, 2);
    }
    for item in &mut items {
        item.cache = CachePolicy::Never;
    }
    frame.replace(size, 1.0, items, Vec::new()).unwrap();
}

fn full_pixels(frame: &PaintFrame) -> Vec<u8> {
    let size = frame.size();
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut renderer = renderer();
    renderer.prepare(frame, RasterMode::Replay);
    let mut p =
        Painter::with_threads(&mut pixels, size.0, size.1, NonZeroU32::new(4).unwrap()).unwrap();
    renderer.paint(frame, &mut p, full_rect(size), Color::rgb(0x18212a));
    p.finish().unwrap();
    pixels
}

#[test]
fn scroll_holes_occlusion_clipping_and_reorder_match_full_render() {
    for buffer_count in [1, 3] {
        let mut buffers: Vec<_> = (0..buffer_count).map(|_| Target::default()).collect();
        let mut frame = PaintFrame::default();
        let mut store = ProgramStore::default();
        let mut reused = 0;
        let mut reordered = 0;
        let mut pixels_copied = 0;
        for step in 0..32 {
            let motion = Motion {
                viewport: Rect {
                    x: 11 + (step % 3) as u32,
                    y: 9,
                    width: 143,
                    height: 110,
                },
                origin: (((step as i32 * 2) % 20) - 3, -((step as i32 * 3) % 25)),
            };
            scene(&mut frame, &mut store, step, motion);
            let target = &mut buffers[step % buffer_count];
            let sample = paint(target, &frame, motion, true);
            reused += usize::from(sample.reason == "reuse");
            reordered += usize::from(sample.reason == "painter-order-changed");
            pixels_copied += sample.copied_pixels;
            assert!(
                target.pixels() == full_pixels(&frame),
                "step {step}, {buffer_count} buffers: {}",
                sample.reason
            );
        }
        assert!(reused > 10 && reordered > 0 && pixels_copied > 0);
    }
}

#[test]
fn fallback_and_failed_frames_do_not_reuse_stale_pixels() {
    let motion = Motion {
        viewport: Rect {
            x: 11,
            y: 9,
            width: 143,
            height: 110,
        },
        origin: (0, 0),
    };
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    scene(&mut frame, &mut store, 0, motion);
    let mut target = Target::default();
    assert_eq!(
        paint(&mut target, &frame, motion, true).reason,
        "new-or-incompatible-target"
    );
    assert_eq!(paint(&mut target, &frame, motion, true).repaired_pixels, 0);
    target.invalidate();
    assert_eq!(
        paint(&mut target, &frame, motion, true).reason,
        "new-or-incompatible-target"
    );
    paint(&mut target, &frame, motion, false);
    assert_eq!(
        paint(&mut target, &frame, motion, true).reason,
        "untracked-target"
    );

    let distant = Motion {
        origin: (1000, 0),
        ..motion
    };
    scene(&mut frame, &mut store, 0, distant);
    assert_eq!(
        paint(&mut target, &frame, distant, true).reason,
        "no-overlap"
    );
    assert!(target.pixels() == full_pixels(&frame));

    let mut other = PaintFrame::default();
    scene(&mut other, &mut store, 0, motion);
    assert_eq!(
        paint(&mut target, &other, motion, true).reason,
        "new-or-incompatible-target"
    );
    let changed = target
        .paint(
            &other,
            motion,
            Color::rgb(0x111111),
            &mut renderer(),
            RasterMode::Replay,
            NonZeroU32::new(2).unwrap(),
            true,
        )
        .unwrap();
    assert_eq!(changed.reason, "new-or-incompatible-target");

    let mut external = other.instances().to_vec();
    external[0].program = store.compile(
        PaintKey::new(1.into(), 1),
        (180.0, 140.0),
        1.0,
        Rc::new(vec![Command::External {
            bounds: Bounds::new(0.0, 0.0, 180.0, 140.0),
            key: 1.into(),
        }]),
    );
    other
        .replace((180, 140), 1.0, external, Vec::new())
        .unwrap();
    assert!(
        target
            .paint(
                &other,
                motion,
                Color::rgb(0x18212a),
                &mut renderer(),
                RasterMode::Replay,
                NonZeroU32::new(2).unwrap(),
                true
            )
            .is_err()
    );
    assert!(target.state.is_none());
    scene(&mut other, &mut store, 0, motion);
    assert_eq!(
        paint(&mut target, &other, motion, true).reason,
        "new-or-incompatible-target"
    );
    assert!(target.pixels() == full_pixels(&other));
}

#[test]
fn translated_panel_works_with_transparent_backdrop_and_rotating_targets() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let mut buffers: Vec<_> = (0..3).map(|_| Target::default()).collect();
    let program = store.compile(
        PaintKey::new(1.into(), 0),
        (70.0, 80.0),
        1.0,
        Rc::new(vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, 70.0, 80.0),
            radius: 7.0,
            color: Color::rgba(30, 50, 90, 170),
        }]),
    );
    for step in 0..20 {
        let origin = ((step * 4) as i32, (step * 2) as i32);
        let viewport = Rect {
            x: origin.0 as u32,
            y: origin.1 as u32,
            width: 70,
            height: 80,
        };
        frame
            .replace(
                (180, 140),
                1.0,
                vec![PaintInstance::new(
                    1.into(),
                    program.clone(),
                    origin,
                    viewport,
                )],
                Vec::new(),
            )
            .unwrap();
        let target = &mut buffers[step % 3];
        let sample = target
            .paint(
                &frame,
                Motion { viewport, origin },
                Color::rgba(0, 0, 0, 0),
                &mut renderer(),
                RasterMode::Replay,
                NonZeroU32::new(4).unwrap(),
                true,
            )
            .unwrap();
        let mut expected = Target::default();
        expected
            .paint(
                &frame,
                Motion { viewport, origin },
                Color::rgba(0, 0, 0, 0),
                &mut renderer(),
                RasterMode::Replay,
                NonZeroU32::new(4).unwrap(),
                false,
            )
            .unwrap();
        assert!(
            target.pixels() == expected.pixels(),
            "step {step}: {}",
            sample.reason
        );
        if step >= 3 {
            assert!(sample.copied_pixels > 0);
        }
    }
}

#[test]
fn stationary_cursor_changes_hover_without_losing_scroll_reuse() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let mut targets: Vec<_> = (0..3).map(|_| Target::default()).collect();
    let viewport = Rect {
        x: 11,
        y: 9,
        width: 143,
        height: 110,
    };
    for step in 0..20 {
        let motion = Motion {
            viewport,
            origin: (0, -(step as i32 * 5)),
        };
        scene(&mut frame, &mut store, 0, motion);
        let hovered = 2 + ((40 - motion.origin.1) / 24) as u64 * 4;
        let mut items = frame.instances().to_vec();
        let item = items.iter_mut().find(|i| i.key == hovered.into()).unwrap();
        item.program = store.compile(
            PaintKey::new(hovered.into(), 1),
            (28.0, 20.0),
            1.0,
            Rc::new(vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 28.0, 20.0),
                radius: 0.0,
                color: Color::rgba(160, 220, 100, 210),
            }]),
        );
        frame.replace((180, 140), 1.0, items, Vec::new()).unwrap();
        let target = &mut targets[step % 3];
        let result = paint(target, &frame, motion, true);
        assert!(target.pixels() == full_pixels(&frame), "hover step {step}");
        if step >= 3 {
            assert!(result.copied_pixels > 0);
            assert!(result.repaired_pixels < area(viewport) * 2 / 3);
        }
    }
}

#[test]
fn rounded_background_reuses_only_proven_uniform_interiors() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let background = store.compile(
        PaintKey::new(1.into(), 0),
        (180.0, 140.0),
        1.0,
        Rc::new(vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, 180.0, 140.0),
            radius: 25.0,
            color: Color::rgba(20, 30, 40, 120),
        }]),
    );
    let viewport = Rect {
        x: 5,
        y: 5,
        width: 160,
        height: 125,
    };
    let mut target = Target::default();
    for step in 0..10 {
        let motion = Motion {
            viewport,
            origin: (0, -(step * 3)),
        };
        scene(&mut frame, &mut store, 0, motion);
        let mut items = frame.instances().to_vec();
        items[0].program = background.clone();
        frame.replace((180, 140), 1.0, items, Vec::new()).unwrap();
        let result = paint(&mut target, &frame, motion, true);
        let expected = full_pixels(&frame);
        // Curved edges repaired with different native clips can round by one channel unit.
        assert!(
            target
                .pixels()
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1)
        );
        if step > 0 {
            assert!(result.copied_pixels > 0);
        }
    }
}
