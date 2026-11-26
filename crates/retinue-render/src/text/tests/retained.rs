use super::*;
use crate::{
    Bounds, Command, PaintFrame, PaintInstance, PaintProgram, ProgramStore, RasterMode, Rect,
    Renderer, ResidencyPolicy, ResourceRead, damage::Region,
};
use std::sync::Arc;

const SIZE: (u32, u32) = (580, 240);

fn viewport() -> Rect {
    Rect {
        width: SIZE.0,
        height: SIZE.1,
        ..Rect::default()
    }
}

fn text(font: Font, count: usize) -> GlyphRun {
    let id = font.metrics.charmap().map_unicode('A').unwrap().to_u32();
    GlyphRun::new(
        font,
        16.0,
        (0..count)
            .map(|i| Glyph {
                id,
                x: i as f64 * 9.25,
                y: 17.75,
            })
            .collect::<Vec<_>>(),
        Hinting::None,
    )
    .unwrap()
}

fn program(store: &mut ProgramStore, run: &GlyphRun, scale: f32) -> PaintProgram {
    store
        .record(
            1,
            (280.0, 35.0),
            scale,
            Arc::new(vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 280.0, 35.0),
                    radius: 3.0,
                    color: Color::rgba(25, 80, 120, 170),
                },
                Command::GlyphRun {
                    origin: (3.25, 4.75),
                    run: run.clone(),
                    color: Color::rgba(210, 238, 190, 220),
                },
            ]),
        )
        .unwrap()
}

fn populate(frame: &mut PaintFrame, program: &PaintProgram, offset: i32) {
    frame
        .replace(
            SIZE,
            program.scale(),
            (0..3)
                .map(|i| {
                    PaintInstance::new(
                        (i as u64 + 1).into(),
                        program.clone(),
                        (offset, 10 + i * 65),
                        viewport(),
                    )
                })
                .collect(),
            Vec::new(),
        )
        .unwrap();
}

fn draw(frame: &PaintFrame, renderer: &mut Renderer) -> Vec<u8> {
    renderer.prepare(frame, RasterMode::Explicit);
    let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
    let mut painter =
        Painter::with_threads(&mut pixels, SIZE.0, SIZE.1, 4.try_into().unwrap()).unwrap();
    renderer.paint(frame, &mut painter, viewport(), Color::rgb(0x18212b));
    painter.finish().unwrap();
    pixels
}

fn same_pixels(a: &[u8], b: &[u8]) {
    assert_eq!(a.len(), b.len());
    assert!(
        a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 1),
        "largest channel difference {}",
        a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap()
    );
}

#[test]
fn publication_tracks_content_separately_from_order_and_handles_skipped_frames() {
    let a = text(font(), 3);
    let b = text(font(), 4);
    let read = |r: &GlyphRun| {
        let (id, index) = r.font().identity();
        ResourceRead::OutlineFont(id, index)
    };
    let (a_read, b_read) = (read(&a), read(&b));
    assert_ne!(a_read, b_read);
    let mut store = ProgramStore::default();
    let (a, b) = (program(&mut store, &a, 1.0), program(&mut store, &b, 1.0));
    let publish = |f: &mut PaintFrame, rows: &[(u64, &PaintProgram)]| {
        f.replace(
            SIZE,
            1.0,
            rows.iter()
                .enumerate()
                .map(|(i, (key, p))| {
                    PaintInstance::new((*key).into(), (*p).clone(), (i as i32 * 8, 0), viewport())
                })
                .collect(),
            vec![],
        )
        .unwrap();
    };
    let mut frame = PaintFrame::default();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    let mut oracle = Renderer::new(ResidencyPolicy::desktop(0));
    oracle.set_optimizations(false);
    publish(&mut frame, &[(1, &a), (2, &a), (3, &b)]);
    same_pixels(&draw(&frame, &mut renderer), &draw(&frame, &mut oracle));
    publish(&mut frame, &[(3, &b), (2, &a), (1, &a)]);
    same_pixels(&draw(&frame, &mut renderer), &draw(&frame, &mut oracle));
    assert_eq!(renderer.stats.content_visits, 0, "order is not content");
    assert_eq!(renderer.stats.prepared_fonts, 0);
    publish(&mut frame, &[(3, &b), (1, &b), (4, &a)]);
    // Desktop chrome can submit another patch before this order reaches paint.
    frame
        .patch_resolved(
            [],
            [crate::PlacementUpdate {
                index: 1,
                origin: (9, 3),
                clip: viewport(),
            }],
            [],
        )
        .unwrap();
    same_pixels(&draw(&frame, &mut renderer), &draw(&frame, &mut oracle));
    assert_eq!(
        renderer.stats.content_visits, 3,
        "one release and two writes"
    );
    assert_eq!(
        renderer.resource_consumers(a_read).collect::<Vec<_>>(),
        [4.into()]
    );
    let mut keys: Vec<_> = renderer.resource_consumers(b_read).collect();
    keys.sort();
    assert_eq!(keys, [1.into(), 3.into()]);
    publish(&mut frame, &[(5, &a)]);
    publish(&mut frame, &[(7, &b)]);
    same_pixels(&draw(&frame, &mut renderer), &draw(&frame, &mut oracle));
    assert_eq!(renderer.resource_consumers(a_read).count(), 0);
    assert_eq!(
        renderer.resource_consumers(b_read).collect::<Vec<_>>(),
        [7.into()]
    );
}

#[test]
fn identical_runs_intern_without_sharing_allocation_or_losing_font_identity() {
    let face = font();
    let run = text(face.clone(), 24);
    let copy = text(face, 24);
    assert_eq!(run, copy);
    assert_ne!(run.glyphs().as_ptr(), copy.glyphs().as_ptr());
    let mut store = ProgramStore::default();
    let a = program(&mut store, &run, 1.2);
    let b = program(&mut store, &copy, 1.2);
    assert_eq!(a.id(), b.id());
    let other = program(&mut store, &text(font(), 24), 1.2);
    assert_ne!(a.id(), other.id());
    assert_eq!(size_of::<GlyphRun>(), size_of::<usize>());
}

#[test]
fn native_runs_are_discovered_and_reused_across_views_without_hints() {
    let face = font();
    let read = ResourceRead::OutlineFont(face.identity().0, face.identity().1);
    for scale in [1.0, 1.2, 1.75] {
        let mut frame = PaintFrame::default();
        let p = program(&mut ProgramStore::default(), &text(face.clone(), 24), scale);
        populate(&mut frame, &p, 8);
        let mut cached = Renderer::new(ResidencyPolicy::desktop(0));
        let mut replay = Renderer::new(ResidencyPolicy::desktop(0));
        replay.set_optimizations(false);
        same_pixels(&draw(&frame, &mut cached), &draw(&frame, &mut replay));
        assert_eq!(cached.stats.prepared_fonts, 1);
        let mut prepared = cached.stats.prepared_fragments;
        for _ in 0..8 {
            if cached.stats.fragment_blits == 3 {
                break;
            }
            same_pixels(&draw(&frame, &mut cached), &draw(&frame, &mut replay));
            assert_eq!(cached.stats.prepared_fonts, 0);
            prepared += cached.stats.prepared_fragments;
        }
        assert_eq!(prepared, 1);
        assert_eq!(cached.stats.fragment_blits, 3);
        assert_eq!(cached.resource_consumers(read).count(), 3);

        cached.prepare_for(&frame, RasterMode::Explicit, &Region::default());
        assert_eq!(cached.stats.prepared_fonts, 0);
        populate(&mut frame, &p, -12);
        same_pixels(&draw(&frame, &mut cached), &draw(&frame, &mut replay));
        assert_eq!(cached.stats.prepared_fonts, 0);
        assert_eq!(cached.stats.prepared_fragments, 0);
        assert_eq!(cached.stats.fragment_blits, 3);
    }
}

#[test]
fn deferred_text_preparation_keeps_replay_correct_and_reuses_font_leases() {
    let face = font();
    let mut store = ProgramStore::default();
    let mut frame = PaintFrame::default();
    let items = (0..3)
        .map(|i| {
            let p = store
                .record(
                    i + 1,
                    (280.0, 35.0),
                    1.0,
                    Arc::new(vec![Command::GlyphRun {
                        origin: (3.25, 4.75),
                        run: text(face.clone(), 24 + i as usize),
                        color: Color::rgb(0xffffff),
                    }]),
                )
                .unwrap();
            PaintInstance::new((i + 1).into(), p, (8, 10 + i as i32 * 65), viewport())
        })
        .collect();
    frame.replace(SIZE, 1.0, items, Vec::new()).unwrap();
    let host = crate::RenderHost::new(crate::HostPolicy {
        preparation_tasks: 1,
        ..crate::HostPolicy::overlay()
    });
    let mut cached = Renderer::new(ResidencyPolicy::desktop(0));
    cached.set_host(host.clone());
    let mut replay = Renderer::new(ResidencyPolicy::desktop(0));
    replay.set_optimizations(false);
    let expected = draw(&frame, &mut replay);
    let mut prepared = 0;
    let mut deferred = false;
    for index in 0..8 {
        same_pixels(&draw(&frame, &mut cached), &expected);
        assert_eq!(cached.stats.prepared_fonts, usize::from(index == 0));
        assert!(host.stats().preparation_tasks <= 1);
        prepared += cached.stats.prepared_fragments;
        deferred |= cached.has_deferred();
    }
    assert!(deferred);
    assert_eq!(prepared, 3);
    assert_eq!(cached.stats.fragment_blits, 3);
    assert!(!cached.has_deferred());
}

#[test]
fn content_changes_reuse_font_leases_and_version_changes_replace_them() {
    let face = font();
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let mut cached = Renderer::new(ResidencyPolicy::desktop(0));
    let mut replay = Renderer::new(ResidencyPolicy::desktop(0));
    replay.set_optimizations(false);
    for (face, count, expected_fonts) in [
        (face.clone(), 24, 1),
        (face.clone(), 18, 0),
        (font(), 18, 1),
    ] {
        let p = program(&mut store, &text(face, count), 1.2);
        populate(&mut frame, &p, 8);
        same_pixels(&draw(&frame, &mut cached), &draw(&frame, &mut replay));
        assert_eq!(cached.stats.prepared_fonts, expected_fonts);
    }
}

#[test]
fn retained_native_queue_survives_owner_and_program_removal() {
    let face = font();
    let weak = face.data().data.downgrade();
    let p = program(&mut ProgramStore::default(), &text(face, 24), 1.0);
    let mut frame = PaintFrame::default();
    populate(&mut frame, &p, 8);
    drop(p);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.set_optimizations(false);
    renderer.prepare(&frame, RasterMode::Replay);
    let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
    let mut painter =
        Painter::with_threads(&mut pixels, SIZE.0, SIZE.1, 4.try_into().unwrap()).unwrap();
    renderer.draw_with(&frame, &mut painter, viewport(), |_, _| unreachable!());
    drop(frame);
    renderer.clear_cache();
    assert!(weak.upgrade().is_some());
    painter.finish().unwrap();
    assert!(weak.upgrade().is_none());
}

#[test]
fn sparse_retained_text_repair_preserves_holes_and_native_fractional_placement() {
    let run = text(font(), 24);
    let origin = (8.25, 5.75);
    for scale in [1.0, 1.2, 1.75] {
        let mut frame = PaintFrame::default();
        let p = PaintProgram::new(
            1,
            (280.0, 35.0),
            scale,
            Arc::new(vec![Command::GlyphRun {
                origin,
                run: run.clone(),
                color: Color::rgb(0xffffff),
            }]),
        )
        .unwrap();
        frame
            .replace(
                SIZE,
                scale,
                vec![PaintInstance::new(1.into(), p, (10, 20), viewport())],
                Vec::new(),
            )
            .unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        renderer.set_optimizations(false);
        let repair = Region::new([
            Rect {
                x: 20,
                y: 20,
                width: 30,
                height: 40,
            },
            Rect {
                x: 100,
                y: 20,
                width: 50,
                height: 60,
            },
        ]);
        renderer.prepare_for(&frame, RasterMode::Replay, &repair);
        let mut actual = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
        let mut painter = Painter::new(&mut actual, SIZE.0, SIZE.1).unwrap();
        painter.repair(&repair);
        renderer.draw_with(&frame, &mut painter, viewport(), |_, _| unreachable!());
        painter.finish().unwrap();
        let mut expected = vec![0; actual.len()];
        let mut painter = Painter::new(&mut expected, SIZE.0, SIZE.1).unwrap();
        painter.repair(&repair);
        let font = PreparedFont::new(run.font(), run.size()).unwrap();
        painter.draw_glyph_run(
            &font,
            &run,
            (
                10.0 + origin.0 as f64 * scale as f64,
                20.0 + origin.1 as f64 * scale as f64,
            ),
            scale as f64,
            Color::rgb(0xffffff),
        );
        painter.finish().unwrap();
        assert_eq!(actual, expected);
        assert!(actual.iter().any(|&v| v != 0));
    }
}
