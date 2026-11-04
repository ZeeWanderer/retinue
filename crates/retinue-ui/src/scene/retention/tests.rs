use super::*;
use crate::layout::{Insets, LayoutAlgorithm, LayoutSize, grid::UniformGrid};
use crate::state::KeyedOrder;
use retinue_render::{
    damage::{AgeRepaintTracker, Region},
    painter::Painter,
};

fn cell(id: u64, frame: usize) -> Element {
    let revision = if id == 3 { (frame % 3) as u64 } else { 0 };
    let mut cell = Element::at(Key::new("card", id), Bounds::new(0.0, 0.0, 38.0, 24.0));
    cell.cache_hint = CacheHint::Stable;
    for part in 0..2 {
        let key = Key::new("paint", id * 2 + part);
        let mut leaf = Element::at(key, Bounds::new(part as f32 * 12.5, 3.0, 27.0, 18.0));
        leaf.paint = (id != 3 || part != 1 || frame % 4 != 2).then(|| {
            Paint::commands(
                PaintKey::new(key, revision),
                Rc::new(vec![Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 27.0, 18.0),
                    radius: 0.0,
                    color: Color::rgba(
                        40 + (id % 10) as u8 * 15,
                        80 + revision as u8 * 20,
                        120,
                        170,
                    ),
                }]),
            )
        });
        leaf.interactive = true;
        cell.children.push(leaf);
    }
    cell
}

fn tree(frame: usize) -> Element {
    let (left, right): (&[u64], &[u64]) = match frame % 6 {
        0 => (&[1, 2, 3], &[4, 5]),
        1 => (&[6, 1, 2, 3], &[4, 5]),
        2 => (&[3, 2, 1], &[4, 5]),
        3 => (&[3, 1], &[2, 4, 5]),
        4 => (&[1, 3], &[4, 5]),
        _ => (&[3, 7, 1], &[5, 4]),
    };
    let grid = UniformGrid::new(
        2,
        LayoutSize::new(38.0, 24.0),
        LayoutSize::new(3.5, 4.5),
        Insets::default(),
    )
    .unwrap();
    let groups = [left, right].into_iter().enumerate().map(|(index, ids)| {
        let mut group = Element::at(
            Key::new("group", index as u64),
            Bounds::new(
                7.0 + index as f32 * 48.5,
                10.0 + index as f32 * 18.5,
                80.0,
                65.0,
            ),
        );
        group.algorithm = Algorithm::Custom(LayoutAlgorithm::new(
            grid.arrange(KeyedOrder::new(ids.iter().map(|&id| Key::new("card", id))).unwrap()),
        ));
        group.children = ids.iter().map(|&id| cell(id, frame)).collect();
        group.clip = frame % 5 != 2;
        group.hit_testing = index != 0 || frame % 4 != 1;
        group.offset = (0.0, -((frame / 2) as f32) * 0.5);
        if index == 1 && frame % 7 == 6 {
            group.style.display = taffy::Display::None;
        }
        group
    });
    let mut root =
        Element::at(Key::new("root", 0), Bounds::new(0.0, 0.0, 150.0, 100.0)).with_children(groups);
    root.offset = (-((frame / 3) as f32) * 0.25, 0.0);
    if frame % 4 == 3 {
        root.children.reverse();
    }
    root
}

fn render(scene: &Scene, renderer: &mut Renderer, pixels: &mut [u8], repair: Region) {
    let size = scene.frame.size();
    renderer.prepare(scene.frame(), RasterMode::Replay);
    let mut painter = Painter::new(pixels, size.0, size.1).unwrap();
    painter.repair(&repair);
    renderer.paint(
        scene.frame(),
        &mut painter,
        repair.bounds(),
        Color::rgb(0x151719),
    );
    painter.finish().unwrap();
}

#[test]
fn structural_projection_matches_fresh_geometry_groups_hits_and_aged_pixels() {
    for scale in [1.0, 1.25, 2.0] {
        let size = ((150.0 * scale) as u32, (100.0 * scale) as u32);
        let mut scene = Scene::default();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(64 * 1024));
        let mut direct = Renderer::new(ResidencyPolicy::desktop(0));
        let mut tracker = AgeRepaintTracker::default();
        let mut buffers = [
            vec![0; (size.0 * size.1 * 4) as usize],
            vec![0; (size.0 * size.1 * 4) as usize],
            vec![0; (size.0 * size.1 * 4) as usize],
        ];
        for frame in 0..20 {
            let damage = scene
                .update(tree(frame), size, scale, RecordingMode::Retained)
                .unwrap();
            let mut oracle = Scene::default();
            oracle
                .update(tree(frame), size, scale, RecordingMode::Always)
                .unwrap();
            assert_eq!(scene.geometry.len(), oracle.geometry.len());
            for (actual, expected) in scene.geometry.iter().zip(&oracle.geometry) {
                let fields = |g: &Geometry| {
                    (
                        g.key,
                        g.parent,
                        g.layout,
                        g.clip,
                        g.hit,
                        g.paint,
                        g.interactive,
                    )
                };
                assert_eq!(
                    fields(actual),
                    fields(expected),
                    "scale={scale}, frame={frame}"
                );
            }
            assert_eq!(
                scene.frame.groups(),
                oracle.frame.groups(),
                "scale={scale}, frame={frame}"
            );
            for y in (0..100).step_by(9) {
                for x in (0..150).step_by(11) {
                    let point = (
                        (x as f64 + 0.5) * scale as f64,
                        (y as f64 + 0.5) * scale as f64,
                    );
                    assert_eq!(scene.hit_test(point), oracle.hit_test(point));
                }
            }
            tracker.invalidate(damage);
            let repair = tracker.plan(if frame < 3 { 0 } else { 3 }, size).unwrap();
            render(
                &scene,
                &mut renderer,
                &mut buffers[frame % 3],
                repair.repair,
            );
            let mut expected = vec![0; buffers[0].len()];
            render(
                &oracle,
                &mut direct,
                &mut expected,
                Damage::Full.regions(size),
            );
            assert_eq!(
                buffers[frame % 3]
                    .iter()
                    .zip(&expected)
                    .position(|(a, b)| a != b),
                None,
                "scale={scale}, frame={frame}"
            );
            tracker.submitted();
        }
    }
}

#[test]
fn structural_insert_skips_geometry_work_in_unchanged_subtrees() {
    let describe = |extra| {
        let stable = Element::at(Key::new("stable", 0), Bounds::new(0.0, 0.0, 80.0, 80.0))
            .with_children((1..30).map(|id| cell(id, 0)));
        let mut root = Element::at(0, Bounds::new(0.0, 0.0, 150.0, 100.0)).with_children([stable]);
        if extra {
            root.children.push(cell(31, 0));
        }
        root
    };
    let mut scene = Scene::default();
    scene
        .update(describe(false), (150, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    scene
        .update(describe(true), (150, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    assert!(
        scene.stats.nodes < 10,
        "projected {} nodes",
        scene.stats.nodes
    );
    assert_eq!(scene.stats.recordings, 2);
}

#[test]
fn child_values_refresh_render_context_without_moving_geometry() {
    use crate::layout::{Layout, LayoutError, Proposal, Subviews, values::LayoutValueKey};
    #[derive(PartialEq)]
    struct Advisory;
    impl LayoutValueKey for Advisory {
        type Value = u32;
        fn default_value() -> u32 {
            0
        }
    }
    impl Layout for Advisory {
        type Cache = ();
        fn measure(
            &self,
            proposal: Proposal,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<LayoutSize, LayoutError> {
            children.measure(0, proposal)
        }
        fn place(
            &self,
            bounds: Bounds,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<(), LayoutError> {
            children.place(0, bounds)
        }
        fn render_context(&self, children: &Subviews<'_, '_>) -> OptimizationContext {
            let value = children.value::<Advisory>(0).unwrap();
            if value == 0 {
                Default::default()
            } else {
                OptimizationContext::default().with(value)
            }
        }
    }
    let mut scene = Scene::default();
    for (iteration, value) in [0, 7, 9, 0].into_iter().enumerate() {
        let mut child = cell(1, 0);
        child.cache_hint = CacheHint::Dynamic;
        child.layout_values = child.layout_values.with::<Advisory>(value);
        let mut root = Element::at(0, Bounds::new(0.0, 0.0, 70.0, 40.0)).with_children([child]);
        root.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Advisory));
        let damage = scene
            .update(root, (100, 60), 1.0, RecordingMode::Retained)
            .unwrap();
        assert_eq!(scene.frame.groups().len(), usize::from(value != 0));
        if let Some(group) = scene.frame.groups().first() {
            assert_eq!(group.context.get::<u32>(), Some(&value));
        }
        if iteration > 0 {
            assert!(matches!(damage, Damage::None));
            assert_eq!(scene.stats.recordings, 0);
        }
    }
}
