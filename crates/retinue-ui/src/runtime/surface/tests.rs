use super::*;
use crate::drawing::PixelLayers;
use retinue_render::{
    CacheHint,
    painter::{GlyphCache, TextLine, load_overlay_font},
};
use std::rc::Rc;

fn render(surface: &mut ViewSurface, recording: PixelLayers) -> (Vec<u8>, Damage) {
    let size = (recording.width(), recording.height());
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut painter = Painter::new(&mut pixels, size.0, size.1).unwrap();
    let damage = surface
        .draw(
            recording.finish(),
            &mut painter,
            Rect {
                width: size.0,
                height: size.1,
                ..Rect::default()
            },
            |_, _| {},
        )
        .unwrap();
    painter.finish().unwrap();
    (pixels, damage)
}

#[test]
fn retained_layers_match_fresh_views_through_move_reorder_resize_and_removal() {
    let mut retained = ViewSurface::default();
    for (width, order, shift, color) in [
        (100, vec![0, 1], 0, 80),
        (100, vec![0, 1], 0, 80),
        (100, vec![0, 1], 0, 160),
        (100, vec![0, 1], 8, 160),
        (120, vec![0, 1], 8, 160),
        (120, vec![1, 0], 8, 160),
        (120, vec![1], 8, 160),
        (120, vec![], 8, 160),
        (120, vec![0, 1], 0, 80),
    ] {
        let record = || {
            let mut recording = PixelLayers::new((width, 80));
            for i in &order {
                let x = 4 + *i as u32 * 10 + shift;
                recording.section(
                    Key::new("item", *i),
                    Rect {
                        x,
                        y: 4,
                        width: 50,
                        height: 50,
                    },
                );
                recording.fill_rounded_rect(x, 4, 50, 50, 4, [color, *i as u8 * 90, 25, 180]);
            }
            recording
        };
        let a = render(&mut retained, record()).0;
        let b = render(&mut ViewSurface::default(), record()).0;
        assert_eq!(a, b);
        assert_eq!(render(&mut retained, record()).1, Damage::None);
        assert_eq!(retained.tree.scene().stats.recordings, 0);
        assert_eq!(retained.tree.scene().stats.layout_computations, 0);
    }
}

#[test]
fn local_pixel_drawing_matches_direct_paint_and_translation_reuses_it() {
    let font = Rc::new(load_overlay_font(&GlyphCache::new(8192)).unwrap());
    let size = (240, 170);
    let mut surface = ViewSurface::default();
    for shift in [0, 9] {
        let mut recording = PixelLayers::new(size);
        recording.section(
            Key::new("card", 0),
            Rect {
                x: 34 + shift,
                y: 23,
                width: 160,
                height: 110,
            },
        );
        recording.draw_ring(65 + shift, 64, 16, 3, [90, 170, 220, 180]);
        recording.draw_text_vertically_centered(
            &font,
            TextLine::new(40 + shift, 80, 20),
            16.0,
            "Relic rewards",
            [255; 4],
        );
        let actual = render(&mut surface, recording).0;
        let mut expected = vec![0; actual.len()];
        let mut painter = Painter::new(&mut expected, size.0, size.1).unwrap();
        painter.draw_ring(65 + shift, 64, 16, 3, [90, 170, 220, 180]);
        painter.draw_text_vertically_centered(
            font.as_ref(),
            TextLine::new(40 + shift, 80, 20),
            16.0,
            "Relic rewards",
            [255; 4],
        );
        painter.finish().unwrap();
        assert_eq!(actual, expected);
        if shift > 0 {
            assert_eq!(surface.tree.scene().stats.recordings, 0);
            assert_eq!(surface.tree.scene().stats.layout_computations, 0);
        }
    }
}

#[test]
fn optional_overlay_layers_are_released_on_close() {
    let mut surface = ViewSurface::with_layers(ResidencyPolicy::overlay(0), LayerPolicy::overlay());
    let bounds = Rect {
        width: 80,
        height: 80,
        ..Rect::default()
    };
    for _ in 0..16 {
        let mut recording = PixelLayers::new((80, 80));
        recording.cached_section(Key::new("card", 0), bounds, CacheHint::Stable);
        for i in 0..64 {
            recording.fill_rounded_rect(i % 8 * 10, i / 8 * 10, 8, 8, 2, [80, 120, 160, 255]);
        }
        render(&mut surface, recording);
    }
    assert!(surface.residency_stats().live_bytes > 0);
    assert!(surface.residency_stats().live_bytes <= ResidencyPolicy::overlay(0).peak_bytes);
    surface.clear_cache();
    assert_eq!(surface.residency_stats().live_bytes, 0);
}

#[test]
fn adding_popup_does_not_rerecord_background() {
    let mut surface = ViewSurface::default();
    for popup in [false, true, false, true] {
        let mut recording = PixelLayers::new((120, 80));
        recording.section(
            Key::new("card", 0),
            Rect {
                x: 10,
                y: 10,
                width: 90,
                height: 60,
            },
        );
        recording.fill_rounded_rect(10, 10, 90, 60, 6, [20, 40, 60, 255]);
        if popup {
            recording.section(
                Key::new("popup", 0),
                Rect {
                    x: 30,
                    y: 20,
                    width: 50,
                    height: 40,
                },
            );
            recording.fill_rounded_rect(30, 20, 50, 40, 4, [180, 70, 20, 180]);
        }
        render(&mut surface, recording);
        assert!(surface.tree.scene().stats.recordings <= 1);
    }
}
