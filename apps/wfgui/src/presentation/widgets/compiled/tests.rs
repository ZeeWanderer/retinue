use super::*;
use crate::presentation::controls::{ControlHost, Weight};
use retinue_render::{RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter};
use retinue_ui::{environment::Environment, ir::InstanceGraph};

fn label_ink(font: Rc<CachedFont>, scale: f32, alignment: f32) -> (u32, u32, u32, u32) {
    let size = ((160.0 * scale) as u32, (40.0 * scale) as u32);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                place(
                    aligned_label("Weapons:", font.clone(), 14.67, [255; 4], alignment),
                    Bounds::new(0.0, 0.0, 160.0, 40.0),
                )
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    graph.advance(size, scale, None).unwrap();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.prepare(graph.frame(), RasterMode::Replay);
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut painter = Painter::new(&mut pixels, size.0, size.1).unwrap();
    renderer.paint(
        graph.frame(),
        &mut painter,
        Damage::Full.bounds(size).unwrap(),
        Color::rgb(0),
    );
    painter.finish().unwrap();
    let mut ink = (size.0, size.1, 0, 0);
    for (i, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[..3].iter().any(|&channel| channel > 32) {
            let (x, y) = (i as u32 % size.0, i as u32 / size.0);
            ink = (ink.0.min(x), ink.1.min(y), ink.2.max(x), ink.3.max(y));
        }
    }
    assert!(ink.0 < ink.2 && ink.1 < ink.3, "text must be visible");
    ink
}

#[test]
fn aligned_labels_use_their_slot_and_keep_vertical_centering() {
    let font = ControlHost::new().unwrap().font_resource(Weight::Regular);
    for scale in [1.0, 1.2, 1.5] {
        let left = label_ink(font.clone(), scale, 0.0);
        let center = label_ink(font.clone(), scale, 0.5);
        let right = label_ink(font.clone(), scale, 1.0);
        assert!(left.0 < center.0 && center.0 < right.0);
        assert!(left.0 <= (2.0 * scale) as u32);
        assert!(right.2 >= (157.0 * scale) as u32);
        assert!(center.0.abs_diff((left.0 + right.0) / 2) <= 1);
        assert_eq!((left.1, left.3), (center.1, center.3));
        assert_eq!((left.1, left.3), (right.1, right.3));
    }
}

#[test]
fn measured_titles_fit_without_elision() {
    use retinue_ui::drawing::{
        TextStyle,
        text::{Paragraph, TextAlignment, text_width},
    };
    let font = ControlHost::new().unwrap().font_resource(Weight::Regular);
    for scale in [1.0, 1.2, 1.5] {
        let title = "Ember Prime";
        let width = text_width(&font, title, 17.0 * scale) / scale;
        let paragraph = Paragraph::new(
            title,
            &TextStyle::new(font.clone(), 17.0, Color::rgb(0xffffff)),
            Some(1),
        );
        assert_eq!(
            paragraph
                .layout(width, TextAlignment::Leading)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [title],
            "scale {scale}, measured {width}, ideal {}",
            paragraph.ideal_width().unwrap()
        );
    }
}
