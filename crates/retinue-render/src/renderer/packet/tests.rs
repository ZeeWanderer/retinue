use super::*;
use crate::{Bounds, Color, Command, PaintInstance, painter::load_overlay_font};
use std::num::NonZeroU32;

#[test]
fn packet_admission_follows_demand_and_publication_not_the_view_type() {
    let mut f = frame(
        1.,
        vec![Command::Fill {
            bounds: Bounds::new(0., 0., 180., 80.),
            radius: 0.,
            color: Color::rgb(0x554433),
        }],
    );
    let clip = f.instances()[0].clip();
    let mut r = Renderer::new(ResidencyPolicy::desktop(0));
    let request = |r: &mut Renderer, f: &PaintFrame, x| {
        let area = Region::from(Rect {
            x,
            y: 0,
            width: 32,
            height: 80,
        });
        r.prepare_for(f, RasterMode::Replay, &area);
        let packet = r.freeze(f, &area, |_, _| unreachable!()).unwrap();
        matches!(packet.draws.as_slice(), [Draw::Shared(_, _)])
    };
    for pos in 1..8 {
        f.patch_resolved(
            [],
            [crate::PlacementUpdate {
                index: 0,
                origin: (pos, 0),
                clip,
            }],
            [],
        )
        .unwrap();
        assert!(
            !request(&mut r, &f, 0),
            "each update replaces the publication"
        );
    }
    assert!(!request(&mut r, &f, 4));
    assert!(request(&mut r, &f, 8));
    assert!(request(&mut r, &f, 12));
    assert!(
        !request(&mut r, &f, 128),
        "unrelated demand must not inherit admission"
    );
    assert!(!request(&mut r, &f, 128));
    assert!(request(&mut r, &f, 128));
    f.patch_resolved(
        [],
        [crate::PlacementUpdate {
            index: 0,
            origin: (8, 0),
            clip,
        }],
        [],
    )
    .unwrap();
    // No reuse before replacement. The next publication starts cold and
    // remembers the wasted admission, but sustained demand still recovers.
    for _ in 0..4 {
        assert!(!request(&mut r, &f, 128));
    }
    assert!(request(&mut r, &f, 128));
    r.trim_inactive();
    assert!(!request(&mut r, &f, 128));
    r.set_optimizations(false);
    for _ in 0..4 {
        assert!(!request(&mut r, &f, 128));
    }
}

#[test]
fn retained_packet_rebuilds_on_resize_or_new_publication() {
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.set_leaf_packet_reuse(true);
    let mut f = frame(
        1.0,
        vec![Command::Fill {
            bounds: Bounds::new(0., 0., 24., 24.),
            radius: 0.,
            color: Color::rgb(0x123456),
        }],
    );
    let clip = f.instances()[0].clip();
    let full: Region = clip.into();
    let small: Region = Rect {
        width: 5,
        height: 5,
        ..clip
    }
    .into();
    let mut packet = |f: &PaintFrame, r: &Region| {
        renderer.prepare_for(f, RasterMode::Replay, r);
        let output = renderer.freeze(f, r, |_, _| unreachable!()).unwrap();
        let [Draw::Shared(child, (0, 0))] = &output.draws[..] else {
            panic!("not retained")
        };
        child.clone()
    };
    let first = packet(&f, &full);
    assert!(Arc::ptr_eq(&first, &packet(&f, &full)));
    let scoped = packet(&f, &small);
    assert!(!Arc::ptr_eq(&first, &scoped));
    f.patch_resolved(
        [],
        [crate::PlacementUpdate {
            index: 0,
            origin: (2, 1),
            clip,
        }],
        [],
    )
    .unwrap();
    let moved = packet(&f, &small);
    assert!(!Arc::ptr_eq(&scoped, &moved));
    assert!(Arc::ptr_eq(&moved, &packet(&f, &small)));
}

#[test]
fn translated_leaf_keeps_order_and_leases_but_not_the_previous_repair() {
    let bounds = Rect {
        width: 64,
        height: 16,
        ..Rect::default()
    };
    let repair = |x| {
        Region::from(Rect {
            x,
            width: 32,
            ..bounds
        })
    };
    let mut f = PaintFrame::default();
    let items = [0, 22, 46]
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            let program = PaintProgram::new(
                i as u64,
                (12., 16.),
                1.,
                Arc::new(vec![Command::Fill {
                    bounds: Bounds::new(0., 0., 12., 16.),
                    radius: 0.,
                    color: Color::rgb(0xff0000 >> (8 * i)),
                }]),
            )
            .unwrap();
            PaintInstance::new((i as u64).into(), program, (x, 0), bounds)
        })
        .collect();
    f.replace((64, 16), 1., items, vec![]).unwrap();
    let freeze = |r: &mut Renderer, area: &Region| {
        r.prepare_for(&f, RasterMode::Replay, area);
        r.freeze(&f, area, |_, _| unreachable!()).unwrap()
    };
    let paint = |packet: &DrawPacket, area: &Region| {
        let mut bytes = vec![18; 64 * 16 * 4];
        let mut p = Painter::new(&mut bytes, 64, 16).unwrap();
        p.repair(area);
        packet.draw(&mut p);
        p.finish().map(|_| bytes)
    };
    fn inner(packet: &DrawPacket) -> &Arc<DrawPacket> {
        let [Draw::Shared(inner, _)] = &packet.draws[..] else {
            panic!("expected retained draws")
        };
        inner
    }
    let mut r = Renderer::new(ResidencyPolicy::desktop(0));
    r.set_leaf_packet_reuse(true);
    let mut oracle = Renderer::new(ResidencyPolicy::desktop(0));
    let _ = freeze(&mut r, &repair(0));
    let swept = freeze(&mut r, &repair(8));
    for x in [16, 0, 8, 16] {
        let packet = freeze(&mut r, &repair(x));
        assert!(Arc::ptr_eq(inner(&swept), inner(&packet)));
        assert!(paint(&packet, &repair(if x == 0 { 8 } else { 0 })).is_err());
        let actual = paint(&packet, &repair(x)).unwrap();
        assert_eq!(
            actual,
            paint(&freeze(&mut oracle, &repair(x)), &repair(x)).unwrap()
        );
        if x == 16 {
            assert_ne!(actual[46 * 4], 18, "newly exposed item was omitted");
        }
    }
    let outside = freeze(&mut r, &repair(28));
    assert!(!Arc::ptr_eq(inner(&swept), inner(&outside)));
}

#[test]
fn whole_item_leases_follow_keys_without_changing_order_or_frozen_packets() {
    let bounds = Rect {
        width: 32,
        height: 16,
        ..Default::default()
    };
    let program = |slot, color| {
        PaintProgram::new(
            slot,
            (12.0, 12.0),
            1.0,
            Arc::new(vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 12.0, 12.0),
                radius: 0.0,
                color: Color::rgb(color),
            }]),
        )
        .unwrap()
    };
    let red = program(1, 0xff0000);
    let blue = program(2, 0x0000ff);
    let item = |key: u64, program: &PaintProgram, x, cache| {
        let mut c = PaintInstance::new(key.into(), program.clone(), (x, 1), bounds);
        c.cache = cache;
        c
    };
    let mut frame = PaintFrame::default();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    let render = |packet: &DrawPacket| {
        let mut pixels = vec![0; 32 * 16 * 4];
        let mut painter = Painter::new(&mut pixels, 32, 16).unwrap();
        let blits = packet.draw(&mut painter).blits;
        painter.finish().unwrap();
        (pixels, blits)
    };
    let check = |r: &mut Renderer, f: &PaintFrame, blits| {
        let full = Region::from(bounds);
        let mut oracle = Renderer::new(ResidencyPolicy::desktop(0));
        oracle.set_optimizations(false);
        oracle.prepare(f, RasterMode::Replay);
        let packet = r.freeze(f, &full, |_, _| unreachable!()).unwrap();
        let expected = render(&oracle.freeze(f, &full, |_, _| unreachable!()).unwrap()).0;
        assert_eq!(render(&packet), (expected.clone(), blits));
        let mut pixels = vec![0; expected.len()];
        let mut p = Painter::new(&mut pixels, 32, 16).unwrap();
        r.draw_with(f, &mut p, bounds, |_, _| unreachable!());
        p.finish().unwrap();
        assert_eq!(pixels, expected);
        packet
    };
    use CachePolicy::{Never, Prefer};
    frame
        .replace(
            (32, 16),
            1.0,
            vec![item(1, &red, 1, Prefer), item(2, &red, 8, Prefer)],
            vec![],
        )
        .unwrap();
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 1);
    let saved = check(&mut renderer, &frame, 2);
    let before = render(&saved).0;
    frame
        .replace(
            (32, 16),
            1.0,
            vec![item(2, &red, 9, Prefer), item(1, &red, 2, Prefer)],
            vec![],
        )
        .unwrap();
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 0);
    check(&mut renderer, &frame, 2);
    frame
        .replace(
            (32, 16),
            1.0,
            vec![item(2, &red, 9, Never), item(1, &blue, 2, Prefer)],
            vec![],
        )
        .unwrap();
    renderer.prepare_for(&frame, RasterMode::Explicit, &Region::default());
    assert_eq!(renderer.stats.prepared_rasters, 0);
    renderer.prepare(&frame, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 1);
    check(&mut renderer, &frame, 1);
    renderer.prepare(&frame, RasterMode::Replay);
    check(&mut renderer, &frame, 0);
    assert_eq!(renderer.residency_stats().selected_bytes, 0);
    assert_eq!(render(&saved).0, before);
}

fn frame(scale: f32, commands: Vec<Command>) -> PaintFrame {
    let mut frame = PaintFrame::default();
    let bounds = Rect {
        width: 260,
        height: 180,
        ..Rect::default()
    };
    frame
        .replace(
            (260, 180),
            scale,
            vec![PaintInstance::new(
                1.into(),
                PaintProgram::new(1, (260.0, 180.0), scale, Arc::new(commands)).unwrap(),
                (0, 0),
                bounds,
            )],
            vec![],
        )
        .unwrap();
    frame
}

#[test]
fn frozen_native_packets_preserve_repair_order_clips_and_child_resources() {
    fn shared<T: Send + Sync>() {}
    shared::<DrawPacket>();
    for scale in [0.9, 1.0, 1.2, 1.5] {
        let font = load_overlay_font().unwrap();
        let child = frame(
            scale,
            vec![
                Command::InsetShadow {
                    bounds: Bounds::new(0.0, 0.0, 80.0, 60.0),
                    radius: 20.0,
                    blur: 4.0,
                    spread: 1.0,
                    color: Color::rgba(20, 140, 80, 200),
                },
                crate::text::fixture::label(
                    &font,
                    "Native packet",
                    16.0,
                    Bounds::new(2.0, 10.0, 100.0, 25.0),
                    crate::TextAlignment::Top,
                    Color::rgb(0xffffff),
                ),
            ],
        );
        let parent = frame(
            scale,
            vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 260.0, 180.0),
                    radius: 0.0,
                    color: Color::rgb(0x102030),
                },
                Command::Clip {
                    bounds: Bounds::new(11.0, 7.0, 180.0, 130.0),
                    commands: Arc::new(vec![
                        Command::External {
                            bounds: Bounds::new(5.0, 13.0, 100.0, 80.0),
                            key: 2.into(),
                        },
                        Command::External {
                            bounds: Bounds::new(48.0, 21.0, 100.0, 80.0),
                            key: 2.into(),
                        },
                    ]),
                },
                Command::Fill {
                    bounds: Bounds::new(36.0, 24.0, 31.0, 85.0),
                    radius: 8.0,
                    color: Color::rgba(200, 30, 70, 150),
                },
            ],
        );
        for repair in [
            Region::from(Rect {
                width: 260,
                height: 180,
                ..Rect::default()
            }),
            Region::new([
                Rect {
                    x: 15,
                    y: 17,
                    width: 17,
                    height: 95,
                },
                Rect {
                    x: 51,
                    y: 3,
                    width: 93,
                    height: 29,
                },
            ]),
        ] {
            let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
            let mut child_renderer = Renderer::new(ResidencyPolicy::desktop(1024 * 1024));
            renderer.prepare_for(&parent, RasterMode::Replay, &repair);
            child_renderer.prepare(&child, RasterMode::Explicit);
            let mut expected = vec![53; 260 * 180 * 4];
            let mut painter = Painter::new(&mut expected, 260, 180).unwrap();
            painter.repair(&repair);
            renderer.draw_with(&parent, &mut painter, repair.bounds(), |_, painter| {
                child_renderer.draw_with(
                    &child,
                    painter,
                    painter.visible_bounds(),
                    |_, _| unreachable!(),
                );
            });
            painter.finish().unwrap();
            let packet = renderer
                .freeze(&parent, &repair, |key, repair| {
                    assert_eq!(key, 2.into());
                    child_renderer.freeze(&child, repair, |_, _| unreachable!())
                })
                .unwrap();
            renderer.clear_cache();
            child_renderer.clear_cache();
            let actual = std::thread::spawn(move || {
                let mut actual = vec![53; 260 * 180 * 4];
                let mut painter =
                    Painter::with_threads(&mut actual, 260, 180, NonZeroU32::new(2).unwrap())
                        .unwrap();
                painter.repair(&repair);
                packet.draw(&mut painter);
                painter.finish().unwrap();
                actual
            })
            .join()
            .unwrap();
            let mismatches = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(8)
                .map(|(i, (a, b))| (i / 4 % 260, i / 4 / 260, *a, *b))
                .collect::<Vec<_>>();
            assert!(mismatches.is_empty(), "scale={scale}: {mismatches:?}");
        }
    }
}
