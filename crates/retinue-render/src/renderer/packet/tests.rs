use super::*;
use crate::{Bounds, Color, Command, PaintInstance, painter::load_overlay_font};
use std::num::NonZeroU32;

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
