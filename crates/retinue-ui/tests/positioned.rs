use retinue_render::{RasterMode, Renderer, ResidencyPolicy, painter::Painter};
use retinue_ui::{
    Bounds,
    layout::positioned::{Placement, Positioned},
    prelude::*,
    runtime::ViewTree,
};

#[derive(PartialEq)]
struct Card(State<Bounds>);

impl View for Card {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            LayoutView(Positioned::new(LayoutSize::new(180.0, 90.0))) {
                Rectangle(Color::rgb(0x111122)).id("background")
                Rectangle(Color::rgb(0xaabbcc)).corner_radius(4.0)
                    .layout_value::<Placement>(Some(self.0.get())).id("image")
            }
        }
    }
}

fn pixels(tree: &ViewTree) -> Vec<u8> {
    let size = tree.frame().size();
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.prepare(tree.frame(), RasterMode::Replay);
    let mut painter = Painter::new(&mut pixels, size.0, size.1).unwrap();
    renderer.draw_with(
        tree.frame(),
        &mut painter,
        retinue_ui::Rect {
            width: size.0,
            height: size.1,
            ..Default::default()
        },
        |_, _| {},
    );
    painter.finish().unwrap();
    pixels
}

#[test]
fn explicit_child_frames_match_wrappers_without_wrapping_each_child() {
    for scale in [1.0, 1.25, 2.0] {
        let size = (400, 300);
        let position = State::new(Bounds::new(10.0, 14.0, 35.0, 28.0));
        let mut tree = ViewTree::new(Card(position.clone()), || {});
        tree.update(size, scale).unwrap();
        let original = tree.unique("image").unwrap().unwrap();
        for bounds in [position.get(), Bounds::new(17.6, 22.4, 48.0, 32.0)] {
            position.set(bounds);
            tree.update(size, scale).unwrap();
            assert_eq!(tree.unique("image").unwrap(), Some(original));
            assert_eq!(tree.stats.mounted, 0);
            assert_eq!(tree.geometry(original).unwrap().layout, bounds);
            let mut reference = ViewTree::new(
                ui! {
                    ZStack() {
                        Rectangle(Color::rgb(0x111122))
                        Rectangle(Color::rgb(0xaabbcc)).corner_radius(4.0)
                            .frame_aligned(bounds.width, bounds.height, retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING)
                            .offset(bounds.x, bounds.y)
                    }.alignment(Alignment::Start).frame(180.0, 90.0)
                },
                || {},
            );
            reference.update(size, scale).unwrap();
            assert!(
                pixels(&tree) == pixels(&reference),
                "scale {scale}, bounds {bounds:?}"
            );
            assert!(tree.scene().geometry().len() < reference.scene().geometry().len());
        }
    }
}

#[test]
fn invalid_child_frames_fail_layout() {
    for bad in [
        Bounds::new(f32::NAN, 0.0, 10.0, 10.0),
        Bounds::new(0.0, 0.0, -10.0, 10.0),
    ] {
        let mut tree = ViewTree::new(Card(State::new(bad)), || {});
        assert!(tree.update((400, 300), 1.0).is_err());
    }
}
