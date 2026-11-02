use super::*;
use crate::{
    Rect,
    damage::{Damage, Region, RepaintTracker},
};

#[test]
fn repair_uses_buffer_coordinates_inside_translated_viewports() {
    let mut pixels = vec![0; 100 * 100 * 4];
    let mut p = Painter::new(&mut pixels, 100, 100).unwrap();
    let a = Rect {
        x: 15,
        y: 25,
        width: 10,
        height: 10,
    };
    let b = Rect { x: 50, y: 60, ..a };
    p.repair(&Region::new([a, b]));
    p.viewport(
        Rect {
            x: 10,
            y: 20,
            width: 70,
            height: 70,
        },
        (10, 20),
        |p| {
            assert_eq!(
                p.visible_regions(),
                Region::new([Rect { x: 5, y: 5, ..a }, Rect { x: 40, y: 40, ..b }])
            );
            p.repair(&a.into());
            p.fill_rounded_rect(0, 0, 80, 80, 0, [20, 40, 70, 255]);
        },
    );
    p.fill_rounded_rect(0, 0, 100, 100, 0, [20, 40, 70, 255]);
    p.finish().unwrap();
    for (i, pixel) in pixels.chunks_exact(4).enumerate() {
        let expected = if a.contains((i % 100) as f64, (i / 100) as f64) {
            [20, 40, 70, 255]
        } else {
            [0; 4]
        };
        assert_eq!(pixel, expected, "pixel {i}");
    }
}

#[test]
fn sparse_repair_preserves_holes_alpha_glyphs_images_and_rotating_buffers() {
    let font = load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap();
    let image = RasterImage::from_rgba(RgbaImage::from_fn(33, 27, |x, y| {
        image::Rgba([(x * 7) as u8, (y * 8) as u8, 175, 127])
    }))
    .unwrap();
    for scale in [1.0_f32, 1.25, 2.0] {
        let px = |x: f32| (x * scale).round() as u32;
        let size = (px(320.0), px(220.0));
        let full = Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let left = Rect {
            x: px(9.0),
            y: px(12.0),
            width: px(48.0),
            height: px(33.0),
        };
        let right = Rect {
            x: px(249.0),
            y: px(165.0),
            width: px(48.0),
            height: px(33.0),
        };
        let draw = |p: &mut Painter<'_>, revision: usize| {
            p.clear();
            p.fill_rounded_rect(0, 0, size.0, size.1, 0, [40, 20, 10, 100]);
            p.fill_rounded_rect(
                px(4.0),
                px(6.0),
                px(308.0),
                px(208.0),
                px(9.0),
                [80, 60, 45, 190],
            );
            p.viewport(
                Rect {
                    x: px(19.0),
                    y: px(36.0),
                    width: px(260.0),
                    height: px(150.0),
                },
                (px(12.0) as i32, -(px(5.0) as i32)),
                |p| {
                    p.draw_image(&image, px(6.0), px(40.0), px(260.0), px(135.0));
                    p.draw_text(
                        &font,
                        px(3.0),
                        px(43.0),
                        22.0 * scale,
                        "Repair clipped text",
                        [220, 240, 230, 210],
                    );
                    p.draw_ring(px(60.0), px(120.0), px(44.0), px(6.0), [200, 80, 30, 160]);
                },
            );
            for (i, r) in [left, right].into_iter().enumerate() {
                p.fill_rounded_rect(
                    r.x,
                    r.y,
                    r.width,
                    r.height,
                    px(5.0),
                    [60, (revision * 15 + i * 50) as u8, 170, 127],
                );
            }
        };
        for threads in [1, 2, 4] {
            let mut buffers: Vec<_> = (0..3)
                .map(|_| vec![0u8; size.0 as usize * size.1 as usize * 4])
                .collect();
            let mut tracker = RepaintTracker::default();
            for _ in &buffers {
                tracker.add_buffer();
            }
            for revision in 0..12 {
                let index = [0, 1, 0, 2, 1][revision % 5];
                tracker.invalidate(
                    Damage::rect(left.expanded(2)).union(Damage::rect(right.expanded(2))),
                );
                let plan = tracker.plan(index, size).unwrap();
                let before = buffers[index].clone();
                let mut p = Painter::with_threads(
                    &mut buffers[index],
                    size.0,
                    size.1,
                    NonZeroU32::new(threads).unwrap(),
                )
                .unwrap();
                p.repair(&plan.repair);
                draw(&mut p, revision);
                p.finish().unwrap();
                tracker.submitted(index);
                let mut expected = vec![0; before.len()];
                let mut p = Painter::with_threads(
                    &mut expected,
                    size.0,
                    size.1,
                    NonZeroU32::new(threads).unwrap(),
                )
                .unwrap();
                draw(&mut p, revision);
                p.finish().unwrap();
                for (i, ((actual, expected), old)) in buffers[index]
                    .chunks_exact(4)
                    .zip(expected.chunks_exact(4))
                    .zip(before.chunks_exact(4))
                    .enumerate()
                {
                    let (x, y) = ((i % size.0 as usize) as u32, (i / size.0 as usize) as u32);
                    assert!(
                        actual
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.abs_diff(*b) <= 1),
                        "{scale} threads={threads} revision={revision} at {x},{y}: {actual:?} != {expected:?}"
                    );
                    if !plan.repair.intersects(Rect {
                        x,
                        y,
                        width: 1,
                        height: 1,
                    }) {
                        assert_eq!(actual, old, "wrote outside repair at {x},{y}");
                    }
                }
                if revision > 4 {
                    assert!(plan.repair.area() < Region::from(full).area() / 4);
                }
            }
        }
    }
}
