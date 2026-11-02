use super::*;
use retinue_render::{RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter};
use retinue_ui::runtime::ViewTree;
use std::time::Instant;

fn placeholder() -> impl IntoElement {
    LoadingPlaceholder {
        color: Color::rgba(120, 140, 160, 64),
        radius: 4.0,
    }
    .frame(80.5, 24.5)
}

fn paint(tree: &ViewTree, renderer: &mut Renderer, pixels: &mut [u8], damage: Damage) {
    let size = tree.frame().size();
    renderer.prepare(tree.frame(), RasterMode::Replay);
    let mut painter = Painter::new(pixels, size.0, size.1).unwrap();
    painter.repair(&damage.regions(size));
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(tree.frame(), &mut painter, bounds, Color::rgb(0x202020));
    }
    painter.finish().unwrap();
}

#[test]
fn pulse_updates_only_paint_and_repairs_match_a_fresh_frame() {
    let start = FrameTime {
        instant: Instant::now(),
        wall_time: UNIX_EPOCH,
    };
    for scale in [1.0, 1.25, 2.0] {
        let size = ((120.0 * scale) as u32, (60.0 * scale) as u32);
        let mut tree = ViewTree::new(placeholder(), || {});
        tree.update_at(size, scale, start).unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        paint(&tree, &mut renderer, &mut pixels, Damage::Full);
        for millis in [16, 350, 700, 1050, 1400] {
            let time = start.at(start.instant + Duration::from_millis(millis));
            let damage = tree.update_at(size, scale, time).unwrap();
            assert_eq!(tree.stats.bodies, 0);
            assert_eq!(tree.stats.mounted, 0);
            assert_eq!(tree.stats.unmounted, 0);
            assert_eq!(tree.scene().stats.layout_computations, 0);
            if millis == 16 {
                assert_eq!(
                    damage,
                    Damage::None,
                    "unchanged display alpha needs no repaint"
                );
                assert_eq!(tree.scene().stats.recordings, 0);
            } else {
                assert_ne!(damage, Damage::None);
            }
            paint(&tree, &mut renderer, &mut pixels, damage);
            let mut expected = ViewTree::new(placeholder(), || {});
            expected.update_at(size, scale, time).unwrap();
            let mut full = vec![0; pixels.len()];
            paint(
                &expected,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut full,
                Damage::Full,
            );
            assert!(pixels == full, "partial repair at {scale}, {millis}ms");
        }
    }
}

#[test]
fn reduced_motion_stops_and_resumes_pulse_deadlines() {
    let time = FrameTime {
        instant: Instant::now(),
        wall_time: UNIX_EPOCH,
    };
    let mut tree = ViewTree::new(placeholder(), || {});
    tree.update_at((120, 60), 1.0, time).unwrap();
    assert!(tree.next_deadline().is_some());
    tree.environment().set::<ReduceMotion>(true);
    tree.update_at((120, 60), 1.0, time).unwrap();
    assert_eq!(tree.next_deadline(), None);
    assert_eq!(tree.scene().stats.layout_computations, 0);
    let later = time.at(time.instant + Duration::from_secs(1));
    assert_eq!(tree.update_at((120, 60), 1.0, later).unwrap(), Damage::None);
    tree.environment().set::<ReduceMotion>(false);
    tree.update_at((120, 60), 1.0, later).unwrap();
    assert!(tree.next_deadline().is_some());
}
