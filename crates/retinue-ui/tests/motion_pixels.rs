use retinue_render::{RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter};
use retinue_ui::{prelude::*, runtime::ViewTree};
use std::time::{Duration, Instant, SystemTime};

#[derive(PartialEq)]
struct Panel {
    x: State<f32>,
    width: State<f32>,
    color: State<Color>,
}
impl View for Panel {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            ZStack() {
                Rectangle(Color::rgb(0x334466)).frame(160.0, 70.0)
                Rectangle(self.color.get()).corner_radius(7.0).frame(self.width.get(), 40.0).offset(self.x.get(), 0.0)
                Text("Foreground").font_size(14.0).offset(0.0, 20.0)
            }.frame(220.0, 100.0).clipped()
        }
    }
}

fn paint(tree: &ViewTree, renderer: &mut Renderer, data: &mut [u8], damage: Damage) {
    let size = tree.frame().size();
    renderer.prepare(tree.frame(), RasterMode::Replay);
    let mut painter = Painter::with_threads(data, size.0, size.1, 4.try_into().unwrap()).unwrap();
    let region = damage.regions(size);
    painter.repair(&region);
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(tree.frame(), &mut painter, bounds, Color::rgb(0x202020));
    }
    painter.finish().unwrap();
}

#[test]
fn animated_partial_repair_matches_independent_layout_and_drawing_at_each_sample() {
    let time = FrameTime {
        instant: Instant::now(),
        wall_time: SystemTime::UNIX_EPOCH,
    };
    let from = Color::rgba(30, 170, 100, 220);
    let to = Color::rgba(200, 80, 70, 180);
    for scale in [1.0, 1.25, 2.0] {
        let size = ((220.0 * scale) as u32, (100.0 * scale) as u32);
        let x = State::new(-40.0);
        let width = State::new(40.0);
        let color = State::new(from);
        let mut tree = ViewTree::new(
            Panel {
                x: x.clone(),
                width: width.clone(),
                color: color.clone(),
            },
            || {},
        );
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        tree.update_at(size, scale, time).unwrap();
        paint(&tree, &mut renderer, &mut pixels, Damage::Full);
        with_animation(
            Animation::new(Duration::from_secs(1)).easing(Easing::Linear),
            || {
                x.set(40.0);
                width.set(80.0);
                color.set(to);
            },
        );
        for millis in [0, 125, 250, 500, 750, 1000] {
            let t = millis as f32 / 1000.0;
            let now = time.at(time.instant + Duration::from_millis(millis));
            let damage = tree.update_at(size, scale, now).unwrap();
            paint(&tree, &mut renderer, &mut pixels, damage);
            let mut expected = ViewTree::new(
                Panel {
                    x: State::new(-40.0 + 80.0 * t),
                    width: State::new(40.0 + 40.0 * t),
                    color: State::new(Color::interpolate(&from, &to, t)),
                },
                || {},
            );
            expected.update_at(size, scale, now).unwrap();
            let mut full = vec![0; pixels.len()];
            paint(
                &expected,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut full,
                Damage::Full,
            );
            assert!(
                pixels == full,
                "animation repair differs at {millis}ms, scale {scale}"
            );
        }
    }
}
