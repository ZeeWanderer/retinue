use super::layout_protocol::{
    self as lp, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews,
};
use super::*;
use crate::layout::anchors::{
    AnchorError, AnchorPoint as Point, AnchorTarget as Target, Anchored, Attachment,
};
use std::cell::Cell;

mod alignment;

fn key(id: u64) -> Key {
    Key::new("layout.test", id)
}

fn leaf(id: u64, width: f32, height: f32) -> Element {
    let mut node = Element::at(key(id), Bounds::new(0.0, 0.0, width, height));
    node.interactive = true;
    node.paint = Some(Paint::new(PaintKey::new(key(id), 0), move |size| {
        vec![Command::Fill {
            bounds: Bounds::new(0.0, 0.0, size.width, size.height),
            radius: 2.0,
            color: Color::rgba(90, id as u8 * 20, 200, 180),
        }]
    }));
    node
}

fn attach(child: u64, target: Target, offset: (f32, f32)) -> Attachment {
    Attachment {
        child: key(child),
        target,
        target_point: Point::BOTTOM_TRAILING,
        child_point: Point::BOTTOM_TRAILING,
        offset,
    }
}

fn container(id: u64, children: Vec<Element>, attachments: Vec<Attachment>) -> Element {
    let mut node =
        Element::at(key(id), Bounds::new(10.0, 10.0, 60.0, 40.0)).with_children(children);
    node.algorithm = Algorithm::Custom(LayoutAlgorithm::new(
        Anchored::new(Point::CENTER, attachments).unwrap(),
    ));
    node
}

fn hosted(node: Element) -> Element {
    Element::at(key(9), Bounds::new(0.0, 0.0, 100.0, 80.0)).with_children([node])
}

#[test]
fn anchors_resolve_dependencies_without_reordering_paint_or_hits() {
    let node = container(
        0,
        vec![leaf(3, 4.0, 4.0), leaf(2, 8.0, 6.0), leaf(1, 20.0, 10.0)],
        vec![
            attach(3, Target::Child(key(2)), (1.0, 1.0)),
            attach(2, Target::Child(key(1)), (2.0, 3.0)),
        ],
    );
    let mut scene = Scene::default();
    scene
        .update(hosted(node), (100, 80), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(
        scene.bounds(key(1)).unwrap().layout,
        Bounds::new(30.0, 25.0, 20.0, 10.0)
    );
    assert_eq!(
        scene.bounds(key(2)).unwrap().layout,
        Bounds::new(44.0, 32.0, 8.0, 6.0)
    );
    assert_eq!(
        scene.bounds(key(3)).unwrap().layout,
        Bounds::new(49.0, 35.0, 4.0, 4.0)
    );
    assert_eq!(
        scene
            .frame
            .instances()
            .iter()
            .map(|item| item.key)
            .collect::<Vec<_>>(),
        [key(3), key(2), key(1)]
    );
    assert_eq!(scene.hit_test((45.0, 33.0)), Some(key(1)));
}

#[test]
fn attachments_do_not_enlarge_intrinsic_size_and_can_overflow_or_clip() {
    let mut node = container(
        0,
        vec![leaf(1, 20.0, 10.0), leaf(2, 30.0, 18.0)],
        vec![attach(2, Target::Container, (2.0, 3.0))],
    );
    node.style.size = Size::auto();
    let mut scene = Scene::default();
    scene
        .update(hosted(node), (100, 80), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(
        scene.bounds(key(0)).unwrap().layout,
        Bounds::new(10.0, 10.0, 20.0, 10.0)
    );
    assert_eq!(
        scene.bounds(key(2)).unwrap().layout,
        Bounds::new(2.0, 5.0, 30.0, 18.0)
    );
    assert_eq!(scene.hit_test((4.0, 6.0)), Some(key(2)));
    scene.patch([Change::Clip(key(0), true)]).unwrap();
    assert_eq!(scene.hit_test((4.0, 6.0)), None);
    assert_eq!(scene.stats.layout_computations, 0);
}

#[test]
fn anchor_diagnostics_identify_cycles_duplicates_and_missing_targets() {
    let a = attach(1, Target::Child(key(2)), (0.0, 0.0));
    let b = attach(2, Target::Child(key(1)), (0.0, 0.0));
    assert_eq!(
        Anchored::new(Point::CENTER, vec![a, b]),
        Err(AnchorError::Cycle(vec![key(1), key(2), key(1)]))
    );
    assert_eq!(
        Anchored::new(Point::CENTER, vec![a, a]),
        Err(AnchorError::DuplicateChild(key(1)))
    );
    assert_eq!(
        Anchored::new(
            Point::CENTER,
            vec![attach(1, Target::Child(key(1)), (0.0, 0.0))]
        ),
        Err(AnchorError::Cycle(vec![key(1), key(1)]))
    );
    assert_eq!(
        Anchored::new(
            Point::CENTER,
            vec![attach(1, Target::Container, (f32::NAN, 0.0))]
        ),
        Err(AnchorError::InvalidGeometry)
    );
    let mut scene = Scene::default();
    scene
        .update(
            container(0, vec![], vec![a]),
            (100, 80),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    assert!(scene.layout_failure().is_none());
    assert!(
        scene
            .update(
                container(0, vec![leaf(1, 10.0, 10.0)], vec![a]),
                (100, 80),
                1.0,
                RecordingMode::Retained
            )
            .is_err()
    );
    assert_eq!(
        scene.layout_failure(),
        Some(&lp::LayoutFailure {
            container: key(0),
            error: LayoutError::MissingChild(key(2))
        })
    );
    scene
        .update(
            container(0, vec![leaf(1, 10.0, 10.0), leaf(2, 5.0, 5.0)], vec![a]),
            (100, 80),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    assert!(scene.layout_failure().is_none());
    assert!(scene.bounds(key(1)).is_some());
}

#[derive(Clone)]
struct Tracked {
    offset: f32,
    updates: Rc<Cell<usize>>,
}

impl PartialEq for Tracked {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset && Rc::ptr_eq(&self.updates, &other.updates)
    }
}

impl lp::Layout for Tracked {
    type Cache = Vec<Key>;
    fn update_cache(
        &self,
        cache: &mut Self::Cache,
        children: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError> {
        cache.clear();
        cache.extend((0..children.len()).map(|i| children.key(i).unwrap()));
        self.updates.set(self.updates.get() + 1);
        Ok(())
    }
    fn measure(
        &self,
        _: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut Self::Cache,
    ) -> Result<LayoutSize, LayoutError> {
        assert_eq!(
            *cache,
            (0..children.len())
                .map(|i| children.key(i).unwrap())
                .collect::<Vec<_>>()
        );
        Ok(LayoutSize::new(40.0, 30.0))
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut Self::Cache,
    ) -> Result<(), LayoutError> {
        for i in 0..children.len() {
            let size = children.measure(i, Proposal::bounded(bounds.width, bounds.height))?;
            children.place(
                i,
                Bounds::new(bounds.x + self.offset, bounds.y, size.width, size.height),
            )?;
        }
        Ok(())
    }
}

#[test]
fn algorithm_equality_and_node_local_cache_keep_unaffected_work_intact() {
    let updates = Rc::new(Cell::new(0));
    let tracked = Tracked {
        offset: 2.0,
        updates: updates.clone(),
    };
    let layout = LayoutAlgorithm::new(tracked.clone());
    let mut a = container(1, vec![leaf(3, 10.0, 10.0)], vec![]);
    let mut b = container(2, vec![leaf(4, 12.0, 10.0)], vec![]);
    a.algorithm = Algorithm::Custom(layout.clone());
    b.algorithm = Algorithm::Custom(layout);
    b.style.inset.left = taffy::LengthPercentageAuto::length(80.0);
    let root =
        Element::at(key(0), Bounds::new(0.0, 0.0, 160.0, 80.0)).with_children([a.clone(), b]);
    let mut scene = Scene::default();
    scene
        .update(root, (160, 80), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(updates.get(), 2);
    let programs: Vec<_> = scene
        .frame
        .instances()
        .iter()
        .map(|i| i.program.id())
        .collect();
    scene
        .patch([Change::Layout(
            key(1),
            a.style.clone(),
            Algorithm::Custom(LayoutAlgorithm::new(tracked.clone())),
        )])
        .unwrap();
    assert_eq!(updates.get(), 2);
    assert_eq!(scene.stats.layout_computations, 0);
    let moved = Tracked {
        offset: 6.0,
        ..tracked
    };
    scene
        .patch([Change::Layout(
            key(1),
            a.style.clone(),
            Algorithm::Custom(LayoutAlgorithm::new(moved)),
        )])
        .unwrap();
    assert_eq!(updates.get(), 3);
    assert_eq!(scene.bounds(key(3)).unwrap().layout.x, 16.0);
    assert_eq!(
        scene
            .frame
            .instances()
            .iter()
            .map(|i| i.program.id())
            .collect::<Vec<_>>(),
        programs
    );
    scene.patch([Change::Interactive(key(3), false)]).unwrap();
    assert_eq!(updates.get(), 3);
    let larger = leaf(3, 18.0, 10.0);
    scene
        .patch([Change::Layout(key(3), larger.style, Algorithm::Flex)])
        .unwrap();
    assert_eq!(updates.get(), 4);
    assert_eq!(scene.bounds(key(3)).unwrap().layout.width, 18.0);
}

#[derive(PartialEq)]
enum Broken {
    Cache,
    Measure,
    Place,
    Proposal,
    Index,
    Omitted,
    MeasurePlace,
}

impl lp::Layout for Broken {
    type Cache = ();
    fn update_cache(&self, _: &mut (), children: &mut Subviews<'_, '_>) -> Result<(), LayoutError> {
        if *self == Self::Cache {
            children.measure(1, Proposal::IDEAL)?;
        }
        Ok(())
    }
    fn measure(
        &self,
        _: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        match self {
            Self::Measure => return Ok(LayoutSize::new(f32::NAN, 10.0)),
            Self::Proposal => {
                children.measure(0, Proposal::bounded(-1.0, 1.0))?;
            }
            Self::Index => {
                children.measure(1, Proposal::IDEAL)?;
            }
            Self::MeasurePlace => children.place(0, Bounds::new(0.0, 0.0, 10.0, 10.0))?,
            _ => {}
        }
        Ok(LayoutSize::new(60.0, 40.0))
    }
    fn place(
        &self,
        _: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        if *self == Self::Omitted {
            return Ok(());
        }
        children.place(0, Bounds::new(f32::INFINITY, 0.0, 10.0, 10.0))
    }
}

#[test]
fn failed_layout_preserves_published_geometry_and_can_recover() {
    for (bad, error) in [
        (Broken::Cache, LayoutError::InvalidChild(1)),
        (Broken::Measure, LayoutError::InvalidGeometry),
        (Broken::Place, LayoutError::InvalidGeometry),
        (Broken::Proposal, LayoutError::InvalidGeometry),
        (Broken::Index, LayoutError::InvalidChild(1)),
        (Broken::Omitted, LayoutError::UnplacedChild(key(1))),
        (
            Broken::MeasurePlace,
            LayoutError::PlacementDuringMeasurement,
        ),
    ] {
        let good = container(0, vec![leaf(1, 10.0, 10.0)], vec![]);
        let mut scene = Scene::default();
        scene
            .update(good.clone(), (100, 80), 1.0, RecordingMode::Retained)
            .unwrap();
        let bounds = scene.bounds(key(1)).unwrap().layout;
        let program = scene.frame.instances()[0].program.id();
        let mut broken = good.clone();
        broken.algorithm = Algorithm::Custom(LayoutAlgorithm::new(bad));
        assert!(
            scene
                .update(broken, (120, 100), 1.25, RecordingMode::Retained)
                .is_err()
        );
        assert_eq!(
            scene.layout_failure(),
            Some(&lp::LayoutFailure {
                container: key(0),
                error
            })
        );
        assert_eq!(scene.frame.size(), (100, 80));
        assert_eq!(scene.frame.scale(), 1.0);
        assert_eq!(scene.bounds(key(1)).unwrap().layout, bounds);
        assert_eq!(scene.frame.instances()[0].program.id(), program);
        assert!(scene.resize((120, 100), 1.25).is_err());
        scene
            .patch([Change::Layout(key(0), good.style, good.algorithm)])
            .unwrap();
        assert!(scene.layout_failure().is_none());
        assert_eq!(scene.bounds(key(1)).unwrap().layout, bounds);
    }
}

#[test]
fn custom_layout_uses_content_bounds_and_resolves_child_padding() {
    let mut node = container(
        0,
        vec![leaf(1, 10.0, 10.0)],
        vec![attach(1, Target::Container, (0.0, 0.0))],
    );
    node.style.padding = taffy::Rect::length(4.0_f32);
    node.style.border = taffy::Rect::length(2.0_f32);
    node.children[0].style.padding = taffy::Rect::percent(0.05_f32);
    let mut scene = Scene::default();
    scene
        .update(hosted(node), (100, 80), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!(
        scene.bounds(key(1)).unwrap().layout,
        Bounds::new(54.0, 34.0, 10.0, 10.0)
    );
    let slot = scene.tree.slot(key(1)).unwrap();
    assert_eq!(
        scene.tree.nodes[slot].as_ref().unwrap().layout.padding.left,
        2.4
    );
}

#[test]
fn layout_caches_drop_with_nodes_or_algorithm_replacement() {
    use std::cell::RefCell;
    #[derive(Clone)]
    struct Probe(Rc<RefCell<std::rc::Weak<()>>>);
    impl PartialEq for Probe {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }
    impl lp::Layout for Probe {
        type Cache = Rc<()>;
        fn update_cache(
            &self,
            cache: &mut Self::Cache,
            _: &mut Subviews<'_, '_>,
        ) -> Result<(), LayoutError> {
            *self.0.borrow_mut() = Rc::downgrade(cache);
            Ok(())
        }
        fn measure(
            &self,
            _: Proposal,
            _: &mut Subviews<'_, '_>,
            _: &mut Self::Cache,
        ) -> Result<LayoutSize, LayoutError> {
            Ok(LayoutSize::ZERO)
        }
        fn place(
            &self,
            _: Bounds,
            _: &mut Subviews<'_, '_>,
            _: &mut Self::Cache,
        ) -> Result<(), LayoutError> {
            Ok(())
        }
    }
    let observed = Rc::new(RefCell::new(std::rc::Weak::new()));
    let mut child = leaf(1, 10.0, 10.0);
    child.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Probe(observed.clone())));
    let mut scene = Scene::default();
    scene
        .update(
            hosted(child.clone()),
            (100, 80),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    assert!(observed.borrow().upgrade().is_some());
    scene
        .patch([Change::Layout(key(1), child.style.clone(), Algorithm::Flex)])
        .unwrap();
    assert!(observed.borrow().upgrade().is_none());
    scene
        .update(hosted(child), (100, 80), 1.0, RecordingMode::Retained)
        .unwrap();
    assert!(observed.borrow().upgrade().is_some());
    scene
        .update(
            leaf(9, 100.0, 80.0),
            (100, 80),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    assert!(observed.borrow().upgrade().is_none());
}
